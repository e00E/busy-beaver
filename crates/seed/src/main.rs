mod enumerate;

use std::{
    io::{BufWriter, Seek, SeekFrom, Write},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use arrayvec::ArrayVec;
use bincode::Options;
use crossbeam_queue::SegQueue;
use enumerate::{
    create_runner, decide, ChildNodes, Decision, HaltingTransitionIndex, Node, States, Transition,
};
use serde::{Deserialize, Serialize};

type Task = (Node, HaltingTransitionIndex);
type TaskResult = (States, Decision);

/// Nodes with up to this many halting transitions are handled locally in thread. Other nodes are handled by the global task queue. The downside of a lower value is higher thread synchronization overhead and higher memory usage and a larger resume file. The upside of a lower value is that individual tasks finish quicker, which gives more fine-grained feedback.
const MAX_LOCAL_HALTING_TRANSITIONS: u8 = 3;

/// One line in the log file is this many bytes including the newline character.
const LOG_ENTRY_LEN: usize = 37;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
struct Stats {
    halt: u64,
    loop_: u64,
    undecided: u64,
    irrelevant: u64,
}

impl Stats {
    fn total(&self) -> u64 {
        self.halt + self.loop_ + self.undecided + self.irrelevant
    }
}

/// Resume data saved on disk.
#[derive(Default, Serialize, Deserialize)]
struct Resume {
    stats: Stats,
    tasks: Vec<Task>,
}

fn main() -> Result<()> {
    let bincode_config = bincode::options();

    let mut resume_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open("resume")
        .context("open resume file")?;
    let resume_len = resume_file
        .metadata()
        .context("read resume file metadata")?
        .len();
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open("log")
        .context("open `log` file")?;
    // Seek to the end because we want to append.
    let log_len = log_file
        .seek(SeekFrom::End(0))
        .context("seek log file to end")?;

    let mut resume: Resume = if resume_len == 0 {
        println!("Resume file has been newly created or was blank. Starting new run.");
        Resume::default()
    } else {
        println!("Resume file exists. Continuing previous run.\nReading resume file.");
        bincode_config
            .deserialize_from(&resume_file)
            .context("deserialize resume file")?
    };

    if (resume.stats.total() == 0) != (resume.tasks.is_empty()) {
        return Err(anyhow!("Resume file stats disagrees with resume file task list about whether this is a fresh run. Try deleting the resume fiel and the log file."));
    }
    let expected_log_len = resume.stats.total() * LOG_ENTRY_LEN as u64;
    if log_len != expected_log_len {
        return Err(anyhow!("The expected number of entries in the log file based on the stats in the resume file do not match the actual number of of entries. Try deleting the resume file and the log file."));
    }

    let keep_running = Arc::new(AtomicBool::new(true));
    let message = "Received request to terminate. Waiting for worker threads to complete their current tasks. This can take a minute. Request termination again to terminate immediately without saving progress.";
    ctrlc::set_handler({
        let keep_running = keep_running.clone();
        let mut first_invocation = true;
        move || {
            if first_invocation {
                println!("{}", message);
                keep_running.store(false, Ordering::Relaxed);
                first_invocation = false;
            } else {
                std::process::exit(1);
            }
        }
    })
    .unwrap();

    // Physical instead of logical core count because in my testing scaling with logical cores falls off.
    let thread_count = num_cpus::get();
    println!("Using {thread_count} threads.");

    // This is the number of active worker threads. A worker thread is either active or inactive. It is active while it handling a task or fetching the next task. It is possible that it turns out that there is no next task but this is still counted as active. A thread is inactive while waiting for a new task to appear.
    //
    // Some threads might temporarily be inactive and become active again when another thread adds more work to the queue. When all threads are inactive we know that the queue is empty and will stay empty.
    let active_threads = Arc::new(AtomicUsize::new(thread_count));
    // Remaining work tasks. Worker threads take tasks from here and put new tasks back.
    let tasks = Arc::new(SegQueue::<Task>::new());
    // Result of a task. Worker threads put items on it and the main thread takes items from it.
    let results = Arc::new(SegQueue::<TaskResult>::new());
    if resume.tasks.is_empty() {
        tasks.push((Node::root(), HaltingTransitionIndex::root()));
        // Replace previous line with the following to test the run quickly completing.
        /*
        let mut states =
            busy_beaver::format::parse_compact(busy_beaver::format::BB5_CHAMPION).unwrap();
        states.0[0][1] = Transition::Halt;
        states.0[1][1] = Transition::Halt;
        states.0[2][1] = Transition::Halt;
        states.0[3][1] = Transition::Halt;
        tasks.push((
            Node(states),
            TransitionIndex(
                crate::enumerate::State::new(0).unwrap(),
                crate::enumerate::Symbol::new(1).unwrap(),
            ),
        ));
        */
    } else {
        // This uses a lot of memory because the vector can only shrink after removing all elements. Fixing that requires reading tasks in a streaming fashion.
        for task in resume.tasks.drain(..) {
            tasks.push(task);
        }
        resume.tasks.shrink_to_fit();
    }

    let start = Instant::now();
    let threads: Vec<JoinHandle<()>> = (0..thread_count)
        .map(|_| {
            let keep_running = keep_running.clone();
            let tasks = tasks.clone();
            let results = results.clone();
            let active_threads = active_threads.clone();
            std::thread::spawn(|| thread_(keep_running, active_threads, tasks, results))
        })
        .collect();

    let mut log_file = BufWriter::new(log_file);
    let mut handle_result = |stats: &mut Stats, result: TaskResult| match result.1 {
        Decision::Halt(_) => {
            stats.halt += 1;
            writeln!(&mut log_file, "{} h", result.0).unwrap();
        }
        Decision::Loop => {
            stats.loop_ += 1;
            writeln!(&mut log_file, "{} l", result.0).unwrap();
        }
        Decision::Undecided => {
            stats.undecided += 1;
            writeln!(&mut log_file, "{} u", result.0).unwrap();
        }
        Decision::Irrelevant => {
            stats.irrelevant += 1;
            writeln!(&mut log_file, "{} i", result.0).unwrap();
        }
    };

    let start_total = resume.stats.total();
    let print_stats = |stats: &Stats, task_queue_len: usize| {
        let elapsed = start.elapsed();
        let seconds_elapsed = elapsed.as_secs_f64();
        let total_enumerated = stats.total();
        let enumerated_per_second_this_run =
            (total_enumerated - start_total) as f64 / elapsed.as_secs_f64();
        println!("seconds elapsed {seconds_elapsed:.1e}, task queue len {task_queue_len:.1e}, total enumerated {total_enumerated:.1e}, enumerated per second this run {enumerated_per_second_this_run:.1e}, {stats:?}");
    };

    println!("Printing initial stats.");
    print_stats(&resume.stats, tasks.len());
    while keep_running.load(Ordering::Relaxed) {
        while let Some(result) = results.pop() {
            handle_result(&mut resume.stats, result);
        }

        // TODO: Double check Ordering. Here and in the thread for this variable. Might have to be SeqCst.
        // TODO: Can't the worker threads check this condition on their own?
        if active_threads.load(Ordering::Relaxed) == 0 {
            keep_running.store(false, Ordering::Relaxed);
            println!("The run is complete. All machines have been enumerated.");
            break;
        }

        print_stats(&resume.stats, tasks.len());

        std::thread::sleep(Duration::from_secs(1));
    }

    for thread in threads {
        thread.join().unwrap();
    }
    println!("Worker threads have finished.");

    println!("Writing remaining logs.");
    let tasks = Arc::into_inner(tasks).unwrap();
    let results = Arc::into_inner(results).unwrap();
    for result in results.into_iter() {
        handle_result(&mut resume.stats, result);
    }
    println!("Printing final stats.");
    print_stats(&resume.stats, tasks.len());
    log_file.flush().context("flush log file")?;

    println!("Writing resume file.");
    assert!(resume.tasks.is_empty());
    resume.tasks.extend(tasks.into_iter());
    resume_file.set_len(0).unwrap();
    resume_file.seek(SeekFrom::Start(0)).unwrap();
    bincode_config
        .serialize_into(&resume_file, &resume)
        .context("write resume file")?;
    resume_file.flush().context("flush resume file")?;

    println!("done");

    Ok(())
}

fn thread_(
    keep_running: Arc<AtomicBool>,
    active_threads: Arc<AtomicUsize>,
    tasks: Arc<SegQueue<Task>>,
    results: Arc<SegQueue<TaskResult>>,
) {
    let mut runner = create_runner();
    'keep_running: while keep_running.load(Ordering::Relaxed) {
        let Some((mut node, branch)) = tasks.pop() else {
            cold();
            active_threads.fetch_sub(1, Ordering::Relaxed);
            while tasks.is_empty() {
                std::thread::sleep(Duration::from_secs_f32(0.1));
                if !keep_running.load(Ordering::Relaxed) {
                    break 'keep_running;
                }
            }
            active_threads.fetch_add(1, Ordering::Relaxed);
            continue;
        };

        let mut stack = ArrayVec::<_, { MAX_LOCAL_HALTING_TRANSITIONS as usize }>::new();
        let element = (ChildNodes::new(&node, branch), branch);
        unsafe { stack.push_unchecked(element) };
        while let Some((nodes, branch)) = stack.last_mut() {
            let Some(transition) = nodes.next() else {
                *node.0.get_transition_mut(branch.0, branch.1) = Transition::Halt;
                let result = stack.pop();
                debug_assert!(result.is_some());
                continue;
            };
            *node.0.get_transition_mut(branch.0, branch.1) = Transition::Continue(transition);
            let decision = decide(&mut runner, &node.0, *branch);
            results.push((node.0, decision));
            if let Decision::Halt(branch) = decision {
                match node.halting_transition_count() {
                    0 | 1 => (),
                    2..=MAX_LOCAL_HALTING_TRANSITIONS => {
                        let element = (ChildNodes::new(&node, branch), branch);
                        unsafe { stack.push_unchecked(element) };
                    }
                    _ => {
                        cold();
                        tasks.push((node, branch));
                    }
                }
            }
        }
    }
    cold();
}

/// Calling this function is a hint to the compiler that this code path is unlikely to be executed.
#[cold]
fn cold() {}

// Optimizations that were tried but did not work out:
//
// Storing the Runner state when branching into child nodes so that they can start their simulation where the previous one left off. This saves simulation steps but has memory and allocation cost. It was slower than rerunning the steps.
//
// An optimization you could do with the knowledge of the previous seed run is to lower the simulation steps to the second best known halting machine. We are not doing this because we want to be able to generate the seed database truly from scratch.

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Read};

    use rayon::{
        prelude::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator},
        slice::ParallelSliceMut,
    };

    use super::*;

    /// Optimized comparison of the logs produced by this binary with the seed database.
    ///
    /// Checks that the following holds for all entries in the log:
    /// 1. If the machine is marked as undecided then it is in the seed database.
    /// 2. If the machine is not marked as undecided then it is not in the seed database.
    ///
    /// On my machine takes 30 seconds to compare all logs.
    #[ignore]
    #[test]
    fn compare_log() {
        // Get this file from http://docs.bbchallenge.org/all_5_states_undecided_machines_with_global_header.zip . Its `shasum` is `2576b647185063db2aa3dc2f5622908e99f3cd40`.
        const SEED_DATABASE_PATH: &str = "all_5_states_undecided_machines_with_global_header.zip";
        let database = std::fs::OpenOptions::new()
            .read(true)
            .open(SEED_DATABASE_PATH)
            .unwrap();
        let log = std::fs::OpenOptions::new().read(true).open("log").unwrap();

        println!("Reading seed database.");
        let mut database = zip::ZipArchive::new(database).unwrap();
        assert_eq!(database.len(), 1);
        let mut database = database.by_index(0).unwrap();
        const DB_ENTRY_LEN: usize = 30;
        const DB_HEADER_LEN: usize = 30;
        // Skip header.
        database.read_exact(&mut [0u8; DB_HEADER_LEN]).unwrap();
        let entries_bytes = database.size() - DB_HEADER_LEN as u64;
        assert!(entries_bytes % DB_ENTRY_LEN as u64 == 0);
        let entries_count = entries_bytes / DB_ENTRY_LEN as u64;
        let mut database_ = Vec::<States>::with_capacity(entries_count as usize);
        let mut buffer = [0u8; 30];
        for _ in 0..entries_count {
            database.read_exact(&mut buffer).unwrap();
            let states = busy_beaver::format::read_seed_database(&buffer).unwrap();
            database_.push(states);
        }
        let mut database = database_;
        println!("Read {} machines.", database.len());

        println!("Sorting machines.");
        database.par_sort_unstable();

        println!("Comparing log.");
        let log_bytes = log.metadata().unwrap().len();
        let mut log = BufReader::new(log);
        assert!(log_bytes % LOG_ENTRY_LEN as u64 == 0);
        let log_count = log_bytes / LOG_ENTRY_LEN as u64;
        const BUFFERED_LOGS_LEN: usize = 1_000_000;
        let mut buf: Vec<[u8; LOG_ENTRY_LEN]> = vec![[0u8; LOG_ENTRY_LEN]; BUFFERED_LOGS_LEN];
        let buf = buf.as_mut_slice();
        let mut buf_pos: usize = 0;
        let handle_logs = |start: u64, logs: &[[u8; LOG_ENTRY_LEN]]| {
            logs.into_par_iter().enumerate().for_each(|(i, log)| {
                let line = start + i as u64 + 1;
                let states = &log[0..34];
                let states = busy_beaver::format::read_compact(states).unwrap();
                let undecided = match log[35] {
                    b'u' => true,
                    b'h' | b'l' | b'i' => false,
                    other => panic!("line {line}, machine {states}, bad character {other}"),
                };
                let undecided_according_to_database = database.binary_search(&states).is_ok();
                assert_eq!(
                    undecided, undecided_according_to_database,
                    "line {line}, machine {states}, {undecided} != {undecided_according_to_database}"
                );
            });
        };
        let mut lines_handled: u64 = 0;
        for _ in 0..log_count {
            if buf_pos == buf.len() {
                handle_logs(lines_handled, buf);
                lines_handled += buf.len() as u64;
                buf_pos = 0;
            }
            log.read_exact(&mut buf[buf_pos]).unwrap();
            buf_pos += 1;
        }
        handle_logs(lines_handled, &buf[..buf_pos]);
        lines_handled += buf_pos as u64;
        assert_eq!(lines_handled, log_count);
        println!("No errors in {log_count} logs.");
    }
}
