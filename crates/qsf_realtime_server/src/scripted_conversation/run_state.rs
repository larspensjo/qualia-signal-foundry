use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeEvent {
    SidebandAttached,
    AttachTimedOut,
    TurnSubmitted {
        index: usize,
    },
    TurnCompleted {
        index: usize,
        promoted: bool,
        elapsed_ms: u64,
        inspection_captured: bool,
        arbitration_winner: Option<String>,
        expectation_differences: Vec<String>,
    },
    TurnTimedOut {
        index: usize,
    },
    SidebandTerminated {
        reason: String,
    },
    FormationBarrierSettled {
        expected: usize,
        settled: usize,
        failed: usize,
        timeout_ms: u64,
    },
    FormationBarrierTimedOut {
        expected: usize,
        settled: usize,
        failed: usize,
        timeout_ms: u64,
    },
    RunFinished,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeTurnState {
    pub index: usize,
    pub submitted: bool,
    pub promoted: Option<bool>,
    pub elapsed_ms: Option<u64>,
    pub timed_out: bool,
    pub inspection_captured: bool,
    pub arbitration_winner: Option<String>,
    pub expectation_differences: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormationState {
    pub expected: usize,
    pub settled: usize,
    pub failed: usize,
    pub timed_out: bool,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeRunState {
    pub attached: bool,
    pub attach_timed_out: bool,
    pub terminated_reason: Option<String>,
    pub turns: Vec<ProbeTurnState>,
    pub formation: Option<FormationState>,
    pub finished: bool,
}

pub fn reduce(state: &mut ProbeRunState, event: ProbeEvent) {
    match event {
        ProbeEvent::SidebandAttached => state.attached = true,
        ProbeEvent::AttachTimedOut => state.attach_timed_out = true,
        ProbeEvent::TurnSubmitted { index } => ensure_turn(state, index).submitted = true,
        ProbeEvent::TurnCompleted {
            index,
            promoted,
            elapsed_ms,
            inspection_captured,
            arbitration_winner,
            expectation_differences,
        } => {
            let turn = ensure_turn(state, index);
            turn.promoted = Some(promoted);
            turn.elapsed_ms = Some(elapsed_ms);
            turn.inspection_captured = inspection_captured;
            turn.arbitration_winner = arbitration_winner;
            turn.expectation_differences = expectation_differences;
        }
        ProbeEvent::TurnTimedOut { index } => ensure_turn(state, index).timed_out = true,
        ProbeEvent::SidebandTerminated { reason } => state.terminated_reason = Some(reason),
        ProbeEvent::FormationBarrierSettled {
            expected,
            settled,
            failed,
            timeout_ms,
        } => {
            state.formation = Some(FormationState {
                expected,
                settled,
                failed,
                timed_out: false,
                timeout_ms,
            });
        }
        ProbeEvent::FormationBarrierTimedOut {
            expected,
            settled,
            failed,
            timeout_ms,
        } => {
            state.formation = Some(FormationState {
                expected,
                settled,
                failed,
                timed_out: true,
                timeout_ms,
            });
        }
        ProbeEvent::RunFinished => state.finished = true,
    }
}

fn ensure_turn(state: &mut ProbeRunState, index: usize) -> &mut ProbeTurnState {
    if let Some(position) = state.turns.iter().position(|turn| turn.index == index) {
        return &mut state.turns[position];
    }
    state.turns.push(ProbeTurnState {
        index,
        ..ProbeTurnState::default()
    });
    state.turns.last_mut().expect("turn was pushed")
}

impl ProbeRunState {
    pub fn promoted_exchange_indices(&self) -> Vec<usize> {
        self.turns
            .iter()
            .filter_map(|turn| (turn.promoted == Some(true)).then_some(turn.index))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{ProbeEvent, ProbeRunState, reduce};

    #[test]
    fn reducer_records_timeout_without_losing_submission() {
        let mut state = ProbeRunState::default();
        reduce(&mut state, ProbeEvent::TurnSubmitted { index: 2 });
        reduce(&mut state, ProbeEvent::TurnTimedOut { index: 2 });
        assert!(state.turns[0].submitted);
        assert!(state.turns[0].timed_out);
    }

    #[test]
    fn reducer_preserves_completion_observations_and_actual_promoted_indices() {
        let mut state = ProbeRunState::default();
        reduce(
            &mut state,
            ProbeEvent::TurnCompleted {
                index: 2,
                promoted: true,
                elapsed_ms: 14,
                inspection_captured: true,
                arbitration_winner: Some("goal".to_string()),
                expectation_differences: vec!["winner".to_string()],
            },
        );
        reduce(
            &mut state,
            ProbeEvent::TurnCompleted {
                index: 0,
                promoted: false,
                elapsed_ms: 9,
                inspection_captured: false,
                arbitration_winner: None,
                expectation_differences: vec![],
            },
        );

        assert_eq!(state.promoted_exchange_indices(), [2]);
        assert_eq!(state.turns[0].arbitration_winner.as_deref(), Some("goal"));
        assert_eq!(state.turns[0].expectation_differences, ["winner"]);
    }
}
