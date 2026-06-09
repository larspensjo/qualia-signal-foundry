use crate::live_state::{LiveSessionEvent, reduce_live_session};
use crate::state::{
    PromptPrefixInvalidation, SessionConfig, SessionEvent, SessionLimit, SessionState,
};

pub fn resume_breaking_config_changed(previous: &SessionConfig, current: &SessionConfig) -> bool {
    previous.model_id != current.model_id
        || previous.max_turns != current.max_turns
        || previous.warm_threshold != current.warm_threshold
        || previous.memory_source != current.memory_source
}

pub fn reduce_session(mut state: SessionState, event: SessionEvent) -> SessionState {
    reduce_session_in_place(&mut state, event);
    state
}

pub fn reduce_session_in_place(state: &mut SessionState, event: SessionEvent) {
    match event {
        SessionEvent::SessionStarted(config) => {
            state.config = config;
        }
        SessionEvent::InputReceived { input } => {
            state.last_input = Some(input);
            state.last_model_error = None;
        }
        SessionEvent::MemoryRetrieved | SessionEvent::ContextAssembled(_) => {}
        SessionEvent::PromptAssembled {
            full_request_hash, ..
        } => {
            state.last_prompt_hash = Some(full_request_hash);
            state.prefix_invalidated_since_last_prompt = false;
        }
        SessionEvent::ModelRoleCompleted { .. } => {
            state.last_model_error = None;
        }
        SessionEvent::ModelRoleFailed { error_summary } => {
            state.last_model_error = Some(error_summary);
        }
        SessionEvent::TurnCompleted(turn) => {
            state.turns.push(turn);
        }
        SessionEvent::PromptPrefixInvalidated {
            after_turn_index,
            reason,
        } => {
            state
                .prompt_prefix_invalidations
                .push(PromptPrefixInvalidation {
                    after_turn_index,
                    reason,
                });
            state.prefix_invalidated_since_last_prompt = true;
        }
        SessionEvent::ExchangeRecorded { exchange, .. } => {
            state.exchanges.push(*exchange);
        }
        SessionEvent::TurnSummarized(summary) => {
            state.summarized_turns.push(summary);
            state.prefix_invalidated_since_last_prompt = true;
        }
        SessionEvent::TurnsAgedAndCoRetrieved {
            range, summaries, ..
        } => {
            debug_assert!(range.last_index >= range.first_index);
            assert_eq!(
                summaries.len(),
                range.last_index + 1 - range.first_index,
                "TurnsAgedAndCoRetrieved summaries must match the aged range"
            );
            state.summarized_turns.extend(summaries);
            state.prefix_invalidated_since_last_prompt = true;
        }
        SessionEvent::ToolCompleted(_) => {}
        SessionEvent::SessionLimitReached {
            current,
            max,
            override_active,
        } => {
            state.limit_reached = Some(SessionLimit {
                current,
                max,
                override_active,
            });
        }
        SessionEvent::SessionEnded { reason } => {
            state.ended_reason = Some(reason);
        }
    }
}

pub fn apply_live_session_event(state: &mut SessionState, event: LiveSessionEvent) {
    let live_state = std::mem::take(&mut state.live);
    state.live = reduce_live_session(live_state, event);
}
