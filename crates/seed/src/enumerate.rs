// This module defines the structure of enumerating turing machines in tree normal form in order to find BB(5). This structure can be used in several ways. One use is the optimized multi threaded version in `main.rs`. Another use is the tests in this module.

use std::hint::unreachable_unchecked;

use busy_beaver::{run::StepResult, states::Direction};
use serde::{Deserialize, Serialize};

// The module could be generic over all kinds of turing machines but for now we only care about 5 symbols, 2 states.

pub type States = busy_beaver::states::States<5, 2>;
pub type State = busy_beaver::states::State<5>;
pub type Symbol = busy_beaver::states::Symbol<2>;
pub type Transition = busy_beaver::states::Transition<5, 2>;
pub type DefinedTransition = busy_beaver::states::DefinedTransition<5, 2>;
pub type Runner = busy_beaver::run::Runner<5, 2, Vec<u8>>;

// The enumeration process builds a tree of turing machines. Every enumerated machines belongs into exactly one of the following categories.

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    /// The machine halts.
    Halt(HaltingTransitionIndex),
    /// The machine runs forever.
    Loop,
    /// The machine could not be decided.
    Undecided,
    /// The machine is irrelevant for finding BB(5).
    Irrelevant,
}

// Each node in the tree that is built by the enumeration process is a turing machine description (an assignment of states).

/// Invariants: The first transition is 1RB. There is at least one halting transition.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Node(pub States);

// The root of the tree is the machine with all halting transitions and 1RB as the first transition.

impl Node {
    pub fn root() -> Self {
        let mut states = busy_beaver::states::States([[Transition::Halt; 2]; 5]);
        states.0[0][0] = Transition::Continue(DefinedTransition {
            write: Symbol::new(1).unwrap(),
            move_: Direction::Right,
            state: State::new(1).unwrap(),
        });
        Self(states)
    }
}

// When running the root node, we see that it encounters a halting transition in the second step. We are going to replace this transition with all possible choices non halting transitions (also called defined transitions). This creates new machines. They are the child nodes of the current node. Child nodes are enumerated in the same fashion until the whole tree is explored.

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HaltingTransitionIndex(pub State, pub Symbol);

impl HaltingTransitionIndex {
    pub fn root() -> Self {
        Self(State::new(1).unwrap(), Symbol::new(0).unwrap())
    }
}

fn assert_invariants(node: &Node, branch: HaltingTransitionIndex) {
    assert_eq!(
        node.0 .0[0][0],
        Transition::Continue(DefinedTransition {
            write: Symbol::new(1).unwrap(),
            move_: Direction::Right,
            state: State::new(1).unwrap(),
        })
    );
    assert_eq!(*node.0.get_transition(branch.0, branch.1), Transition::Halt);
    assert!((2..=9).contains(&node.halting_transition_count()));
    assert!((0..=5).contains(&node.largest_partially_defined_state().get()));
}

// The enumeration can be expressed as a recursive function as seen below. Here we use `trace` as a callback for every enumerated machine. `trace` can also inform the recursion to stop early, which is useful for testing.
//
// Machines that do not halt are leaf nodes. They do not have child nodes. The recursion ends with them. The remaining halting transitions do not need to be explored because they are unreachable.
//
// This function enumerates the machines in the same order as the seed run when go-routines are disabled. This is useful for testing.

#[allow(dead_code)]
#[inline(always)]
fn enumerate_recursively(
    mut node: Node,
    branch: HaltingTransitionIndex,
    runner: &mut Runner,
    trace: &mut impl FnMut(&States, Decision) -> bool,
) -> bool {
    for transition in ChildNodes::new(&node, branch) {
        *node.0.get_transition_mut(branch.0, branch.1) = Transition::Continue(transition);
        let decision = decide(runner, &node.0, branch);
        if trace(&node.0, decision) {
            crate::cold();
            return true;
        }
        if let Decision::Halt(branch) = decision {
            // There is no point in continuing with 1 halting transition. In the next step it would be turned into a non halting transition, which would leave the machine with no halting transition.
            if node.halting_transition_count() >= 2 {
                let stop = enumerate_recursively(node, branch, runner, trace);
                if stop {
                    return true;
                }
            }
        }
    }
    false
}

// The enumeration can be expressed iteratively instead of recursively. This function enumerates the machines in the same order.

#[allow(dead_code)]
#[inline(always)]
fn enumerate_iteratively(
    mut node: Node,
    branch: HaltingTransitionIndex,
    runner: &mut Runner,
    trace: &mut impl FnMut(&States, Decision) -> bool,
) {
    let mut stack = arrayvec::ArrayVec::<_, 8>::new();
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
        let decision = decide(runner, &node.0, *branch);
        if trace(&node.0, decision) {
            crate::cold();
            return;
        }
        if let Decision::Halt(branch) = decision {
            if node.halting_transition_count() >= 2 {
                let element = (ChildNodes::new(&node, branch), branch);
                unsafe { stack.push_unchecked(element) };
            }
        }
    }
}

// There are some things we commonly want to know about the current node.

impl Node {
    // For a larger number of total states it might be worth it to include `halting_transition_count`, `largest_partially_defined_state` in the node instead of computing them on demand. It takes constant time to compute the next value from the previous value for the recursion.

    #[inline(always)]
    pub fn halting_transition_count(&self) -> u8 {
        self.0
             .0
            .iter()
            .flatten()
            .fold(0, |acc, t| acc + (*t == Transition::Halt) as u8)
    }

    #[inline(always)]
    pub fn largest_partially_defined_state(&self) -> State {
        let result = self
            .0
             .0
            .iter()
            .enumerate()
            .rev()
            .find(|(_, state)| (state[0] != Transition::Halt) | (state[1] != Transition::Halt))
            .map(|(i, _)| unsafe { State::new_unchecked(i as u8) });
        unsafe { result.unwrap_unchecked() }
    }
}

// Each enumerated machine is categorized by the following function. It takes the runner as an argument instead of creating one from scratch every time. This is more efficient.

#[inline(never)]
pub fn decide(
    runner: &mut Runner,
    states: &States,
    changed_transition: HaltingTransitionIndex,
) -> Decision {
    if is_irrelevant(states, changed_transition.0, changed_transition.1) {
        crate::cold();
        return Decision::Irrelevant;
    }
    runner.set_states(states);
    runner.reset();
    run(runner)
}

// A machine is irrelevant when it does not needed to be ran in order to find BB(5).

/// `changed_state` is the state which was modified in `states` to arrive at this machine. Knowing it allows us to be more efficient and not repeat checks that were already done before.
#[inline(always)]
fn is_irrelevant(states: &States, changed_state: State, read: Symbol) -> bool {
    has_equivalent_states(states, changed_state)
        || has_redundant_transition(states, changed_state, read)
}

#[inline(always)]
fn has_equivalent_states(states: &States, changed_state: State) -> bool {
    (0u8..5).any(|i| {
        i != changed_state.get()
            && are_states_defined_and_equivalent(
                states,
                unsafe { State::new_unchecked(i) },
                changed_state,
            )
    })
}

#[inline(always)]
fn are_states_defined_and_equivalent(states: &States, a: State, b: State) -> bool {
    let a_ = states.get_state(a);
    let b_ = states.get_state(b);
    let (
        [Transition::Continue(a0), Transition::Continue(a1)],
        [Transition::Continue(b0), Transition::Continue(b1)],
    ) = (a_, b_)
    else {
        return false;
    };
    (a0.write == b0.write)
        & (a0.move_ == b0.move_)
        & (a1.write == b1.write)
        & (a1.move_ == b1.move_)
        & ((a0.state == b0.state)
            | (((a0.state == a) | (a0.state == b)) & ((b0.state == b) | (b0.state == a))))
        & ((a1.state == b1.state)
            | (((a1.state == a) | (a1.state == b)) & ((b1.state == b) | (b1.state == a))))
}

#[inline(always)]
fn has_redundant_transition(states: &States, changed_state: State, read: Symbol) -> bool {
    let Transition::Continue(t) = states.get_transition(changed_state, read) else {
        debug_assert!(false, "unreachable");
        unsafe { unreachable_unchecked() };
    };
    let [Transition::Continue(n0), Transition::Continue(n1)] = states.get_state(t.state) else {
        return false;
    };
    let copies = (n0.write.get() == 0) & (n1.write.get() == 1);
    let moves_back = (n0.move_ != t.move_) & (n1.move_ != t.move_);
    let states_back = n0.state == n1.state;
    copies & moves_back & states_back
}

// When running a turing machine, we need to stop eventually in case it runs forever. These limits are given by the following constants. If they are reached, the machine is undecided.

const LIMIT_STEPS: u32 = 47176870;
const LIMIT_MEMORY: isize = 12289;
const TAPE_SIZE: usize = LIMIT_MEMORY as usize * 2;

// While running we can detect some cases of never halting through the known limits of BB(4).

const BB4_STEPS: u32 = 107;
#[allow(dead_code)]
const BB4_SPACE: isize = 16;

pub fn create_runner() -> Runner {
    Runner::vector_backed(TAPE_SIZE)
}

// This function is the most important factor in the speed of the enumeration process. Many machines are run until the step or space limit is reached. In order to optimize this function, some changes were made from the seed run:
//
// Exact tape space limits have been removed. The original code checks used space against BB4 and conjectured BB5. We remove this check because we already have a space limit check in `Runner`. This check is less precise because the total tape size is two times the conjectured space limit. The loss in precision is made up by faster execution speed. For machines that are decided as non halting by the BB4 space limit this doesn't change correctness because any machine decided as non halting by the BB4 space limit will also be decided as non halting by the BB4 step limit. There could be a change in behavior compared to the original code if a machine halts while using more space than the conjectured BB5 space limit and less space than our less precise space limit. In this case the original code would treat the machine as undecided while this code would treat it as halting.

#[inline(always)]
fn run(runner: &mut Runner) -> Decision {
    let mut state_seen: u8 = 0;
    let mut step: u32 = 0;
    loop {
        state_seen |= 1 << runner.state().get();
        let all_states_seen = state_seen == 0b00011111;
        // Moving this here is faster than any other place. I am not sure why. It might influence how the compiler can rewrite the loop because `step()` happening here is an observable side effect.
        let result = runner.step();
        let bb4_exceeded = (!all_states_seen) & (step > BB4_STEPS);
        if bb4_exceeded {
            crate::cold();
            return Decision::Loop;
        }
        let bb5_exceeded = step > LIMIT_STEPS;
        if bb5_exceeded {
            crate::cold();
            return Decision::Undecided;
        }
        step += 1;
        match result {
            StepResult::Ok => (),
            StepResult::Halt => {
                crate::cold();
                return Decision::Halt(HaltingTransitionIndex(runner.state(), runner.symbol()));
            }
            StepResult::TapeFullLeft | StepResult::TapeFullRight => {
                crate::cold();
                return Decision::Undecided;
            }
        }
    }
}

/// Iterator over a halting node's child nodes.
pub struct ChildNodes {
    exhausted: bool,
    max_state: u8,
    symbol: u8,
    direction: u8,
    state: u8,
}

impl ChildNodes {
    #[inline(always)]
    pub fn new(node: &Node, branch: HaltingTransitionIndex) -> Self {
        if cfg!(debug_assertions) {
            assert_invariants(node, branch);
        }

        let largest_partially_defined_state = node
            .largest_partially_defined_state()
            .get()
            .max(branch.0.get());
        let target_states_end = (largest_partially_defined_state + 1).min(4);

        Self {
            exhausted: false,
            max_state: target_states_end,
            state: 0,
            direction: 0,
            symbol: 0,
        }
    }
}

impl Iterator for ChildNodes {
    type Item = DefinedTransition;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            crate::cold();
            return None;
        }
        let result = DefinedTransition {
            state: unsafe { State::new_unchecked(self.state) },
            move_: match self.direction {
                0 => Direction::Right,
                1 => Direction::Left,
                _ => {
                    debug_assert!(false, "unreachable");
                    unsafe { unreachable_unchecked() }
                }
            },
            write: unsafe { Symbol::new_unchecked(self.symbol) },
        };
        self.exhausted = true;
        for (current, max) in [&mut self.symbol, &mut self.direction, &mut self.state]
            .into_iter()
            .zip([1, 1, self.max_state])
        {
            if *current < max {
                self.exhausted = false;
                *current += 1;
                break;
            } else {
                *current = 0;
            }
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, Write},
        time::Instant,
    };

    use super::*;

    // Test that traces an execution and compares it with a previously recorded trace.

    fn write_trace(mut out: impl Write, states: &States, trace: Decision) -> std::io::Result<()> {
        let trace = match trace {
            Decision::Halt(..) => "Halt",
            Decision::Loop => "Loop",
            Decision::Undecided => "Undecided",
            Decision::Irrelevant => "Irrelevant",
        };
        writeln!(&mut out, "{states} {trace}")
    }

    #[ignore]
    #[test]
    fn create_trace() {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open("trace.txt")
            .unwrap();
        let mut writer = std::io::BufWriter::new(&mut file);
        let mut callback = |states: &_, trace| {
            write_trace(&mut writer, states, trace).unwrap();
        };
        enumerate_for_tests(&mut callback, 1500);
        writer.flush().unwrap();
    }

    #[ignore]
    #[test]
    fn compare_trace() {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open("trace.txt")
            .unwrap();
        let mut reader = std::io::BufReader::new(file);
        let mut expected = Vec::<u8>::new();
        let mut actual = Vec::<u8>::new();
        let mut i: u64 = 0;
        let mut callback = |states: &_, trace| {
            i += 1;

            expected.clear();
            reader.read_until(b'\n', &mut expected).unwrap();
            expected.pop().unwrap();

            actual.clear();
            write_trace(&mut actual, states, trace).unwrap();
            actual.pop().unwrap();

            if actual != expected {
                let actual = std::str::from_utf8(actual.as_slice()).unwrap();
                let expected = std::str::from_utf8(expected.as_slice()).unwrap();
                println!("Line {i} does not match.\nexpected: {expected}\nactual  : {actual}");
                panic!();
            }
        };
        enumerate_for_tests(&mut callback, 1500);
        assert_eq!(i, 1500);
        // Should be at end of file.
        let bytes_read = reader.read_until(b'\n', &mut expected).unwrap();
        assert_eq!(bytes_read, 0);
    }

    #[ignore]
    #[test]
    fn speedtest() {
        let start = Instant::now();
        enumerate_for_tests(&mut |_, _| (), 1500);
        let end = start.elapsed();
        println!("{:.1e}", end.as_secs_f32());
    }

    /// Initiate the enumeration procedure and run until `steps` machines have been enumerated.
    fn enumerate_for_tests(trace: &mut impl FnMut(&States, Decision), steps: u64) {
        let mut step: u64 = 0;
        let mut trace = |states: &States, decision: Decision| {
            trace(states, decision);
            step += 1;
            step >= steps
        };
        enumerate_iteratively(
            Node::root(),
            HaltingTransitionIndex::root(),
            &mut Runner::vector_backed(TAPE_SIZE),
            &mut trace,
        );
    }
}
