use crate::states::States;

#[derive(Debug)]
pub enum Decision {
    Halt,
    RunForever,
    Irrelevant,
    Undecided,
}

pub trait Decider {
    fn decide(&mut self, states: &States<5, 2>) -> Decision;
}
