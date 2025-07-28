use rust_fsm::*;

#[derive(Clone, Debug, PartialEq)]
pub enum RracerState {
    Waiting,
    Countdown,
    Racing,
    Finished,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RracerEvent {
    Join,
    CountdownElapsed,
    AllDone,
    Reset,
}

impl StateMachineImpl for RracerState {
    type Input = RracerEvent;
    type State = RracerState;
    
    fn transition(state: &Self::State, input: &Self::Input) -> Option<Self::State> {
        match (state, input) {
            (RracerState::Waiting, RracerEvent::Join) => Some(RracerState::Countdown),
            (RracerState::Countdown, RracerEvent::CountdownElapsed) => Some(RracerState::Racing),
            (RracerState::Racing, RracerEvent::AllDone) => Some(RracerState::Finished),
            (RracerState::Finished, RracerEvent::Reset) => Some(RracerState::Waiting),
            _ => None,
        }
    }
}

impl Default for RracerState {
    fn default() -> Self {
        RracerState::Waiting
    }
}
