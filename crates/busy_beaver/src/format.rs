//! Turing machine formatting

use anyhow::{anyhow, Context, Result};
use arrayvec::ArrayVec;

use crate::states::{DefinedTransition, Direction, State, States, Symbol, Transition};

pub const BB5_CHAMPION_COMPACT: &[u8] = b"1RB1LC_1RC1RB_1RD0LE_1LA1LD_---0LA";
pub const BB4_CHAMPION_COMPACT: &[u8] = b"1RB1LB_1LA0LC_---1LD_1RD0RA_------";

/// Parse a compact human readable turing machine representation.
pub fn read_compact(s: &[u8]) -> Result<States<5, 2>> {
    if s.len() != 34 {
        return Err(anyhow!("invalid length"));
    }
    let states = s
        .chunks(7)
        .map(|s| {
            s.chunks_exact(3)
                .map(read_transition_compact)
                .collect::<Result<ArrayVec<_, 2>>>()
                .map(|a| a.into_inner().unwrap())
        })
        .collect::<Result<ArrayVec<_, 5>>>()?
        .into_inner()
        .unwrap();
    Ok(States(states))
}

fn read_transition_compact(s: &[u8]) -> Result<Transition<5, 2>> {
    assert_eq!(s.len(), 3);
    if s == b"---" {
        return Ok(Transition::Halt);
    }
    let write = Symbol::new(s[0] - b'0').context("invalid symbol")?;
    let move_ = match s[1] {
        b'L' => Direction::Left,
        b'R' => Direction::Right,
        _ => return Err(anyhow!("invalid move direction")),
    };
    let state = State::new(s[2] - b'A').context("invalid state")?;
    Ok(Transition::Continue(DefinedTransition {
        write,
        move_,
        state,
    }))
}

/// Parse a Bbchallenge seed database turing machine representation.
pub fn read_seed_database(s: &[u8]) -> Result<States<5, 2>> {
    if s.len() != 30 {
        return Err(anyhow!("invalid length"));
    }
    let mut states = States::default();
    for (chunk, transition) in s.chunks_exact(3).zip(states.0.iter_mut().flatten()) {
        *transition = read_transition_seed_database(chunk)?;
    }
    Ok(states)
}

fn read_transition_seed_database(s: &[u8]) -> Result<Transition<5, 2>> {
    assert_eq!(s.len(), 3);
    if s == [0, 0, 0] {
        return Ok(Transition::Halt);
    }
    let write = Symbol::new(s[0]).context("invalid symbol")?;
    let move_ = match s[1] {
        0 => Direction::Right,
        1 => Direction::Left,
        _ => return Err(anyhow!("invalid move direction")),
    };
    let state = State::new(s[2] - 1).context("invalid state")?;
    Ok(Transition::Continue(DefinedTransition {
        write,
        move_,
        state,
    }))
}

impl std::fmt::Display for States<5, 2> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, state) in self.0.iter().enumerate() {
            if i != 0 {
                write!(f, "_")?;
            }
            for transition in state {
                let Transition::Continue(DefinedTransition {
                    write,
                    move_,
                    state,
                }) = transition
                else {
                    write!(f, "---")?;
                    continue;
                };
                let write = char::from_u32(b'0' as u32 + write.get() as u32).unwrap();
                let direction = match move_ {
                    Direction::Left => 'L',
                    Direction::Right => 'R',
                };
                let state = char::from_u32(b'A' as u32 + state.get() as u32).unwrap();
                write!(f, "{write}{direction}{state}")?;
            }
        }
        Ok(())
    }
}

/// Write a turing machine in Bbchallenge seed database representation.
pub fn write_seed_database(states: &States<5, 2>) -> [u8; 30] {
    let mut result = [0u8; 30];
    for (transition, chunk) in states.0.iter().flatten().zip(result.chunks_exact_mut(3)) {
        match transition {
            Transition::Halt => chunk.copy_from_slice(&[0; 3]),
            Transition::Continue(t) => {
                chunk[0] = t.write.get();
                chunk[1] = match t.move_ {
                    Direction::Left => 1,
                    Direction::Right => 0,
                };
                chunk[2] = t.state.get() + 1;
            }
        }
    }
    result
}

#[test]
fn parse_bb5_champion() {
    let states = read_compact(BB5_CHAMPION_COMPACT).unwrap();
    assert_eq!(BB5_CHAMPION_COMPACT, states.to_string().as_bytes());
}

#[test]
fn database() {
    let database = &[
        1u8, 0, 2, 0, 1, 4, 0, 1, 3, 1, 1, 5, 1, 1, 4, 1, 1, 3, 0, 0, 1, 0, 0, 0, 1, 0, 2, 1, 0, 5,
    ];
    let compact = b"1RB0LD_0LC1LE_1LD1LC_0RA---_1RB1RE";
    let a = read_seed_database(database).unwrap();
    let b = read_compact(compact).unwrap();
    assert_eq!(a, b);
    let a = write_seed_database(&a);
    assert_eq!(database, &a);
}
