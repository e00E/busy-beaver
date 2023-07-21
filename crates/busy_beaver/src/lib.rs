pub mod decider;
pub mod format;
pub mod normalize;
pub mod run;
pub mod states;

/// Calling this function is a hint to the compiler that this code path is unlikely to be executed.
#[cold]
fn cold() {}
