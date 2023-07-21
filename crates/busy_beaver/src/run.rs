//! Optimized turing machine running

// This module uses a custom state representation as an optimization.
//
// The only change is that `enum Direction` stores the tape position offset directly.
//
// I also tried a branchless version which worked like this:
// - Create another Direction variant for keeping the head in place by using a 0 offset.
// - Create a sixth state that is used as the halting state. This state does not do any modifications. It keeps the in place, writes the same symbol back, goes to itself.
// - Convert halting transitions into transitions into this state.
// - Loop for a fixed number of steps: the step number of the BB(5) champion. Checking this is the only branch.
// - In the loop do the usual state transition through look up table, which is now branchless because halting does not need to be detected.
// - Optionally the tape can be detected as full and reads out of bounds prevented by doing something like `let pos_ = pos; pos = pos.max(0); pos = pos.min(ape.len()); is_full |= pos_ != pos;`.
// Despite resulting in simpler assembly with less instructions and less branches, the program runs slower for BB(5), which is the best case for this adapted algorithm. Machines that halt earlier have less benefit because the new algorithm doesn't exit early on halting. It even runs slower when removing the tape out of bounds check. Unrolling the loop did not help either.

use crate::states::{DefinedTransition, Direction, State, States, Symbol, Transition};

#[derive(Clone)]
pub struct Runner<const STATES: usize, const SYMBOLS: usize, Storage> {
    states: [[Transition_; SYMBOLS]; STATES],
    state: u8,
    tape: Tape<Storage>,
}

impl<const STATES: usize, const SYMBOLS: usize> Runner<STATES, SYMBOLS, Vec<u8>> {
    pub fn vector_backed(length: usize) -> Self {
        Self::new(vec![0u8; length])
    }
}

impl<const STATES: usize, const SYMBOLS: usize, const LENGTH: usize>
    Runner<STATES, SYMBOLS, [u8; LENGTH]>
{
    pub fn array_backed() -> Self {
        Self::new([0u8; LENGTH])
    }
}

impl<const STATES: usize, const SYMBOLS: usize, Storage> Runner<STATES, SYMBOLS, Storage>
where
    Storage: AsRef<[u8]> + AsMut<[u8]>,
{
    pub fn new(storage: Storage) -> Self {
        assert!(STATES > 0);
        Self {
            states: [[Transition_::default(); SYMBOLS]; STATES],
            state: 0,
            tape: Tape::new(storage),
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.state = 0;
        self.tape.reset();
    }

    #[inline(always)]
    pub fn set_states(&mut self, states: &States<STATES, SYMBOLS>) {
        self.states = states.0.map(|s| s.map(Self::map_transition));
    }

    #[inline(always)]
    pub fn set_transition(
        &mut self,
        state: State<STATES>,
        symbol: Symbol<SYMBOLS>,
        transition: Transition<STATES, SYMBOLS>,
    ) {
        let state = unsafe { self.states.get_unchecked_mut(state.get() as usize) };
        let transition_ = unsafe { state.get_unchecked_mut(symbol.get() as usize) };
        *transition_ = Self::map_transition(transition);
    }

    fn map_transition(transition: Transition<STATES, SYMBOLS>) -> Transition_ {
        match transition {
            Transition::Halt => Transition_::Halt,
            Transition::Continue(DefinedTransition {
                write,
                move_,
                state,
            }) => Transition_::Continue {
                write: write.get(),
                move_: match move_ {
                    Direction::Left => Direction_::Left,
                    Direction::Right => Direction_::Right,
                },
                state: state.get(),
            },
        }
    }

    #[inline(always)]
    pub fn state(&self) -> State<STATES> {
        unsafe { State::new_unchecked(self.state) }
    }

    #[inline(always)]
    pub fn symbol(&self) -> Symbol<SYMBOLS> {
        let s = self.tape.read();
        unsafe { Symbol::new_unchecked(s) }
    }

    /// When the head of the tape moves out of bounds the current transition is still applied but the head is not moved.
    #[inline(always)]
    pub fn step(&mut self) -> StepResult<STATES, SYMBOLS> {
        let symbol = self.tape.read() as usize;
        let state = self.state as usize;
        debug_assert!(self.states.get(state).is_some());
        let state = unsafe { self.states.get_unchecked(state) };
        debug_assert!(state.get(symbol).is_some());
        let transition = *unsafe { state.get_unchecked(symbol) };
        match transition {
            Transition_::Halt => {
                crate::cold();
                StepResult::Halt
            }
            Transition_::Continue {
                write,
                move_,
                state,
            } => {
                self.tape.write(write);
                self.state = state;
                match self.tape.move_(move_) {
                    Ok(()) => StepResult::Ok,
                    Err(OutOfBounds::Left) => {
                        crate::cold();
                        StepResult::TapeFullLeft
                    }
                    Err(OutOfBounds::Right) => {
                        crate::cold();
                        StepResult::TapeFullRight
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StepResult<const STATES: usize, const SYMBOLS: usize> {
    Ok,
    Halt,
    TapeFullLeft,
    TapeFullRight,
}

#[derive(Clone, Copy, Default)]
enum Transition_ {
    #[default]
    Halt,
    Continue {
        write: u8,
        move_: Direction_,
        state: u8,
    },
}

#[derive(Clone, Copy)]
#[repr(isize)]
enum Direction_ {
    Left = -1,
    Right = 1,
}

#[derive(Clone)]
struct Tape<Storage> {
    storage: Storage,
    // invariant: valid index into tape
    pos: isize,
}

impl<Storage> Tape<Storage>
where
    Storage: AsRef<[u8]> + AsMut<[u8]>,
{
    fn new(storage: Storage) -> Self {
        let len = storage.as_ref().len();
        assert!(len > 0);
        let len: isize = len.try_into().unwrap();
        Self {
            storage,
            pos: len / 2,
        }
    }

    #[inline(always)]
    fn reset(&mut self) {
        for s in self.storage.as_mut().iter_mut() {
            *s = 0;
        }
        self.pos = (self.storage.as_ref().len() / 2).try_into().unwrap();
    }

    #[inline(always)]
    fn read(&self) -> u8 {
        let storage = self.storage.as_ref();
        debug_assert!(storage.get(self.pos as usize).is_some());
        *unsafe { storage.get_unchecked(self.pos as usize) }
    }

    #[inline(always)]
    fn write(&mut self, symbol: u8) {
        let storage = self.storage.as_mut();
        debug_assert!(storage.get_mut(self.pos as usize).is_some());
        *unsafe { storage.get_unchecked_mut(self.pos as usize) } = symbol;
    }

    /// Returns whether the move would result in the position being out of bounds. In that case no move is performed.
    #[allow(clippy::result_unit_err)]
    #[inline(always)]
    fn move_(&mut self, direction: Direction_) -> Result<(), OutOfBounds> {
        let new_pos = self.pos.wrapping_add(direction as isize);
        if new_pos < 0 {
            crate::cold();
            Err(OutOfBounds::Left)
        } else if new_pos >= self.storage.as_ref().len() as isize {
            crate::cold();
            Err(OutOfBounds::Right)
        } else {
            self.pos = new_pos;
            Ok(())
        }
    }
}

enum OutOfBounds {
    Left,
    Right,
}

#[test]
#[ignore]
fn speedtest() {
    let states = crate::format::read_compact(crate::format::BB5_CHAMPION_COMPACT).unwrap();
    let mut run = Runner::vector_backed(30_000);
    // let mut run = Runner::<5, 2, [u8; 30_000]>::array_backed();
    run.set_states(&states);
    let start = std::time::Instant::now();
    let mut steps: u64 = 0;
    loop {
        steps += 1;
        match run.step() {
            StepResult::Ok => {}
            other => {
                let elapsed = start.elapsed();
                println!("{other:?} time {elapsed:?} steps {steps}");
                break;
            }
        }
    }
}
