//! Type safe turing machine description

use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct States<const STATES: usize, const SYMBOLS: usize>(
    // `serde_as` is needed for the serialization derives because serde cannot handle generic arrays.
    #[serde_as(as = "[[_; SYMBOLS]; STATES]")] pub [[Transition<STATES, SYMBOLS>; SYMBOLS]; STATES],
);

impl<const STATES: usize, const SYMBOLS: usize> Default for States<STATES, SYMBOLS> {
    fn default() -> Self {
        Self([[Transition::default(); SYMBOLS]; STATES])
    }
}

impl<const STATES: usize, const SYMBOLS: usize> States<STATES, SYMBOLS> {
    #[inline(always)]
    pub fn get_state(&self, state: State<STATES>) -> &[Transition<STATES, SYMBOLS>; SYMBOLS] {
        let index = state.get() as usize;
        debug_assert!(self.0.get(index).is_some());
        unsafe { self.0.get_unchecked(index) }
    }

    #[inline(always)]
    pub fn get_state_mut(
        &mut self,
        state: State<STATES>,
    ) -> &mut [Transition<STATES, SYMBOLS>; SYMBOLS] {
        let index = state.get() as usize;
        debug_assert!(self.0.get(index).is_some());
        unsafe { self.0.get_unchecked_mut(index) }
    }

    #[inline(always)]
    pub fn get_transition(
        &self,
        state: State<STATES>,
        symbol: Symbol<SYMBOLS>,
    ) -> &Transition<STATES, SYMBOLS> {
        let state_ = self.get_state(state);
        let index = symbol.get() as usize;
        debug_assert!(state_.get(index).is_some());
        unsafe { state_.get_unchecked(index) }
    }

    #[inline(always)]
    pub fn get_transition_mut(
        &mut self,
        state: State<STATES>,
        symbol: Symbol<SYMBOLS>,
    ) -> &mut Transition<STATES, SYMBOLS> {
        let state_ = self.get_state_mut(state);
        let index = symbol.get() as usize;
        debug_assert!(state_.get(index).is_some());
        unsafe { state_.get_unchecked_mut(index) }
    }
}

/// Invariant: Inner value is smaller than COUNT.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct State<const COUNT: usize>(u8);

impl<const COUNT: usize> State<COUNT> {
    #[inline(always)]
    pub fn new(state: u8) -> Option<Self> {
        if state as usize >= COUNT {
            return None;
        }
        Some(Self(state))
    }

    #[allow(clippy::missing_safety_doc)]
    #[inline(always)]
    pub unsafe fn new_unchecked(state: u8) -> Self {
        debug_assert!(Self::new(state).is_some());
        Self(state)
    }

    #[inline(always)]
    pub fn get(&self) -> u8 {
        self.0
    }
}

/// Invariant: Inner value is smaller than COUNT.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Symbol<const COUNT: usize>(u8);

impl<const COUNT: usize> Symbol<COUNT> {
    #[inline(always)]
    pub fn new(symbol: u8) -> Option<Self> {
        if symbol as usize >= COUNT {
            return None;
        }
        Some(Self(symbol))
    }

    #[allow(clippy::missing_safety_doc)]
    #[inline(always)]
    pub unsafe fn new_unchecked(symbol: u8) -> Self {
        debug_assert!(Self::new(symbol).is_some());
        Self(symbol)
    }

    #[inline(always)]
    pub fn get(&self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Transition<const STATES: usize, const SYMBOLS: usize> {
    #[default]
    Halt,
    Continue(DefinedTransition<STATES, SYMBOLS>),
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DefinedTransition<const STATES: usize, const SYMBOLS: usize> {
    pub write: Symbol<SYMBOLS>,
    pub move_: Direction,
    pub state: State<STATES>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Ord, PartialOrd, Serialize, Deserialize)]
#[repr(u8)]
pub enum Direction {
    #[default]
    Left,
    Right,
}
