//! Turing machine normalization

use arrayvec::ArrayVec;

use crate::states::{DefinedTransition, Direction, State, States, Transition};

pub fn is_normal<const STATES: usize, const SYMBOLS: usize>(d: &States<STATES, SYMBOLS>) -> bool {
    // TODO:
    // - Enforce first write is 1?
    // - Enforce halt transitions at end and only 1 halt transition?
    // - Enforce that non blank symbols first occur in ascending order. This is true for all 2 symbol machines.

    first_transition_moves_right(d) && non_initial_states_first_occur_in_ascending_order(d)
}

pub fn normalize<const STATES: usize, const SYMBOLS: usize>(d: &mut States<STATES, SYMBOLS>) {
    if !first_transition_moves_right(d) {
        reverse_directions(d);
        debug_assert!(first_transition_moves_right(d));
    }
    if !non_initial_states_first_occur_in_ascending_order(d) {
        order_states(d);
        debug_assert!(non_initial_states_first_occur_in_ascending_order(d));
    }
    debug_assert!(is_normal(d));
}

fn first_transition_moves_right<const STATES: usize, const SYMBOLS: usize>(
    d: &States<STATES, SYMBOLS>,
) -> bool {
    let Some(move_) =
        d.0.iter()
            .flatten()
            .filter_map(|t| match t {
                Transition::Halt => None,
                Transition::Continue(DefinedTransition { move_, .. }) => Some(*move_),
            })
            .next()
    else {
        return true;
    };
    move_ == Direction::Right
}

fn reverse_directions<const STATES: usize, const SYMBOLS: usize>(d: &mut States<STATES, SYMBOLS>) {
    for move_ in d.0.iter_mut().flatten().filter_map(|t| match t {
        Transition::Halt => None,
        Transition::Continue(DefinedTransition { move_, .. }) => Some(move_),
    }) {
        *move_ = match move_ {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        };
    }
}

fn non_initial_states_first_occur_in_ascending_order<const STATES: usize, const SYMBOLS: usize>(
    d: &States<STATES, SYMBOLS>,
) -> bool {
    order_in_which_non_initial_states_occur(d)
        .as_slice()
        .windows(2)
        .all(|states| states[0] < states[1])
}

fn order_states<const STATES: usize, const SYMBOLS: usize>(d: &mut States<STATES, SYMBOLS>) {
    let actual_order = order_in_which_non_initial_states_occur(d);
    let target_order = {
        let mut o = actual_order.clone();
        o.sort();
        o
    };
    for (a, b) in actual_order.iter().zip(target_order.iter()) {
        swap_states(d, *a, *b);
    }
}

fn order_in_which_non_initial_states_occur<const STATES: usize, const SYMBOLS: usize>(
    d: &States<STATES, SYMBOLS>,
) -> ArrayVec<State<STATES>, STATES> {
    d.0.iter()
        .flatten()
        .filter_map(|t| match t {
            Transition::Halt => None,
            Transition::Continue(DefinedTransition { state, .. }) => Some(*state),
        })
        .filter(|s| *s != State::new(0).unwrap())
        .collect()
}

fn swap_states<const STATES: usize, const SYMBOLS: usize>(
    d: &mut States<STATES, SYMBOLS>,
    a: State<STATES>,
    b: State<STATES>,
) {
    d.0.swap(a.get() as usize, b.get() as usize);
    for state in d.0.iter_mut().flatten().filter_map(|t| match t {
        Transition::Halt => None,
        Transition::Continue(DefinedTransition { state, .. }) => Some(state),
    }) {
        if *state == a {
            *state = b;
        } else if *state == b {
            *state = a;
        }
    }
}
