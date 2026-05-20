use crate::session::{SessionConfig, SessionLimit, SessionState};

/// Prepare a persisted `SessionState` for awake continuation in a new run.
pub fn prepare_awake_continuation(
    previous: SessionState,
    new_config: &SessionConfig,
) -> SessionState {
    let mut state = previous;
    state.config = new_config.clone();
    state.ended_reason = None;
    state.last_input = None;
    state.last_model_error = None;
    state.last_prompt_hash = None;
    state.prefix_invalidated_since_last_prompt = true;
    state.previous_session_id = None;
    state.limit_reached = recompute_limit(&state, new_config);
    state
}

fn recompute_limit(state: &SessionState, new_config: &SessionConfig) -> Option<SessionLimit> {
    let current = state.turns.len();
    if current >= new_config.max_turns {
        Some(SessionLimit {
            current,
            max: new_config.max_turns,
            override_active: new_config.allow_over_limit,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{MemorySourceConfig, SessionEndReason};

    fn config(max_turns: usize) -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig {
                source: "fixture".to_string(),
                file: None,
            },
        }
    }

    fn previous_with(end_reason: Option<SessionEndReason>, turns: usize) -> SessionState {
        let mut state = SessionState::new_with_id("prev-1".to_string(), config(10));
        for index in 0..turns {
            state.turns.push(crate::session::tests::fake_turn(index));
        }
        state.ended_reason = end_reason;
        state.last_input = Some("hello".to_string());
        state.last_model_error = Some("network blip".to_string());
        state.last_prompt_hash = Some(crate::conversation::ContentHash([1; 32]));
        state
    }

    #[test]
    fn after_quit_command_clears_end_reason_and_last_input_and_error() {
        let prev = previous_with(Some(SessionEndReason::QuitCommand), 2);
        let cont = prepare_awake_continuation(prev, &config(10));

        assert_eq!(cont.ended_reason, None);
        assert_eq!(cont.last_input, None);
        assert_eq!(cont.last_model_error, None);
        assert_eq!(cont.session_id, "prev-1");
        assert_eq!(cont.turns.len(), 2);
    }

    #[test]
    fn after_eof_clears_state_same_way() {
        let prev = previous_with(Some(SessionEndReason::Eof), 1);
        let cont = prepare_awake_continuation(prev, &config(10));

        assert_eq!(cont.ended_reason, None);
        assert_eq!(cont.last_model_error, None);
    }

    #[test]
    fn after_model_error_clears_error_field() {
        let prev = previous_with(Some(SessionEndReason::Error), 3);
        let cont = prepare_awake_continuation(prev, &config(10));

        assert_eq!(cont.ended_reason, None);
        assert_eq!(cont.last_model_error, None);
    }

    #[test]
    fn limit_reached_recomputes_against_new_config_under_limit() {
        let mut prev = previous_with(Some(SessionEndReason::QuitCommand), 5);
        prev.limit_reached = Some(SessionLimit {
            current: 5,
            max: 5,
            override_active: false,
        });

        let cont = prepare_awake_continuation(prev, &config(10));
        assert_eq!(cont.limit_reached, None);
    }

    #[test]
    fn limit_reached_recomputes_when_still_over_new_limit() {
        let prev = previous_with(Some(SessionEndReason::QuitCommand), 12);
        let cont = prepare_awake_continuation(prev, &config(10));

        assert_eq!(
            cont.limit_reached,
            Some(SessionLimit {
                current: 12,
                max: 10,
                override_active: false,
            })
        );
    }

    #[test]
    fn prefix_cache_is_invalidated_in_new_run() {
        let prev = previous_with(Some(SessionEndReason::QuitCommand), 1);
        let cont = prepare_awake_continuation(prev, &config(10));

        assert_eq!(cont.last_prompt_hash, None);
        assert!(cont.prefix_invalidated_since_last_prompt);
    }
}
