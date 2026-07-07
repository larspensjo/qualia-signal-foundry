//! Session-scoped token ledger for the diagnostics page. Aggregates provider-reported
//! token usage per (role, model) into class counters (fresh text/audio input, cached
//! input, text/audio output). Raw counts only - no price table, no dollar conversion
//! (see the DecisionLog entry on the diagnostics token meter).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenClassCounts {
    pub text_input: u64,
    pub audio_input: u64,
    pub cached_input: u64,
    pub text_output: u64,
    pub audio_output: u64,
}

impl TokenClassCounts {
    pub fn add_assign_saturating(&mut self, other: TokenClassCounts) {
        self.text_input = self.text_input.saturating_add(other.text_input);
        self.audio_input = self.audio_input.saturating_add(other.audio_input);
        self.cached_input = self.cached_input.saturating_add(other.cached_input);
        self.text_output = self.text_output.saturating_add(other.text_output);
        self.audio_output = self.audio_output.saturating_add(other.audio_output);
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelTokenUsage {
    pub model_id: String,
    pub role: String,
    pub calls: u32,
    pub counts: TokenClassCounts,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenUsageSnapshot {
    pub qsf_session_id: String,
    pub models: Vec<ModelTokenUsage>,
}

impl TokenUsageSnapshot {
    pub fn new(qsf_session_id: String) -> Self {
        Self {
            qsf_session_id,
            models: Vec::new(),
        }
    }

    pub fn record(&mut self, role: &str, model_id: &str, counts: TokenClassCounts) {
        if let Some(row) = self
            .models
            .iter_mut()
            .find(|row| row.role == role && row.model_id == model_id)
        {
            row.calls = row.calls.saturating_add(1);
            row.counts.add_assign_saturating(counts);
            return;
        }
        self.models.push(ModelTokenUsage {
            model_id: model_id.to_string(),
            role: role.to_string(),
            calls: 1,
            counts,
        });
    }
}

pub(crate) fn usage_number(event: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = event.get("response")?.get("usage")?;
    for segment in path {
        current = current.get(segment)?;
    }
    current.as_u64()
}

pub(crate) fn response_done_token_counts(event: &serde_json::Value) -> TokenClassCounts {
    let input = usage_number(event, &["input_tokens"]).unwrap_or(0);
    let cached = usage_number(event, &["input_token_details", "cached_tokens"])
        .or_else(|| usage_number(event, &["cached_input_tokens"]))
        .unwrap_or(0);
    let output = usage_number(event, &["output_tokens"]).unwrap_or(0);

    let input_text = usage_number(event, &["input_token_details", "text_tokens"]);
    let input_audio = usage_number(event, &["input_token_details", "audio_tokens"]);
    let cached_text = usage_number(
        event,
        &[
            "input_token_details",
            "cached_tokens_details",
            "text_tokens",
        ],
    )
    .unwrap_or(0);
    let cached_audio = usage_number(
        event,
        &[
            "input_token_details",
            "cached_tokens_details",
            "audio_tokens",
        ],
    )
    .unwrap_or(0);
    let output_text = usage_number(event, &["output_token_details", "text_tokens"]);
    let output_audio = usage_number(event, &["output_token_details", "audio_tokens"]);

    let (text_input, audio_input) = match (input_text, input_audio) {
        (None, None) => (input.saturating_sub(cached), 0),
        (text, audio) => {
            let mut fresh_text = text.unwrap_or(0).saturating_sub(cached_text);
            let mut fresh_audio = audio.unwrap_or(0).saturating_sub(cached_audio);
            let mut remainder = cached.saturating_sub(cached_text.saturating_add(cached_audio));
            let from_text = remainder.min(fresh_text);
            fresh_text -= from_text;
            remainder -= from_text;
            fresh_audio = fresh_audio.saturating_sub(remainder);
            (fresh_text, fresh_audio)
        }
    };
    let (text_output, audio_output) = match (output_text, output_audio) {
        (None, None) => (output, 0),
        (text, audio) => (text.unwrap_or(0), audio.unwrap_or(0)),
    };

    TokenClassCounts {
        text_input,
        audio_input,
        cached_input: cached,
        text_output,
        audio_output,
    }
}

#[cfg(test)]
mod tests {
    use qsf_realtime_protocol::OPENAI_REALTIME_VOICE_MODEL;

    use super::*;

    #[test]
    fn detailed_usage_splits_audio_text_and_cached() {
        let event = serde_json::json!({
            "response": {
                "usage": {
                    "total_tokens": 1000,
                    "input_tokens": 900,
                    "output_tokens": 100,
                    "input_token_details": {
                        "text_tokens": 300,
                        "audio_tokens": 600,
                        "cached_tokens": 500,
                        "cached_tokens_details": { "text_tokens": 200, "audio_tokens": 300 }
                    },
                    "output_token_details": { "text_tokens": 20, "audio_tokens": 80 }
                }
            }
        });
        assert_eq!(
            response_done_token_counts(&event),
            TokenClassCounts {
                text_input: 100,
                audio_input: 300,
                cached_input: 500,
                text_output: 20,
                audio_output: 80,
            }
        );
    }

    #[test]
    fn missing_details_fall_back_to_text_classes() {
        let partial = serde_json::json!({
            "response": {
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 4,
                    "input_token_details": { "text_tokens": 8, "cached_tokens": 3 }
                }
            }
        });
        assert_eq!(
            response_done_token_counts(&partial),
            TokenClassCounts {
                text_input: 5,
                audio_input: 0,
                cached_input: 3,
                text_output: 4,
                audio_output: 0,
            }
        );

        let bare = serde_json::json!({
            "response": { "usage": { "input_tokens": 10, "output_tokens": 4, "cached_input_tokens": 3 } }
        });
        assert_eq!(
            response_done_token_counts(&bare),
            TokenClassCounts {
                text_input: 7,
                audio_input: 0,
                cached_input: 3,
                text_output: 4,
                audio_output: 0,
            }
        );

        assert_eq!(
            response_done_token_counts(&serde_json::json!({})),
            TokenClassCounts::default()
        );
    }

    #[test]
    fn cached_without_cached_details_never_double_counts_input() {
        let spilling = serde_json::json!({
            "response": {
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 1,
                    "input_token_details": { "text_tokens": 3, "audio_tokens": 7, "cached_tokens": 5 }
                }
            }
        });
        let counts = response_done_token_counts(&spilling);
        assert_eq!(
            counts,
            TokenClassCounts {
                text_input: 0,
                audio_input: 5,
                cached_input: 5,
                text_output: 1,
                audio_output: 0,
            }
        );
        assert!(counts.text_input + counts.audio_input + counts.cached_input <= 10);
    }

    #[test]
    fn record_accumulates_per_role_model_and_keeps_first_seen_order() {
        let mut snapshot = TokenUsageSnapshot::new("session-test".to_string());
        let counts = TokenClassCounts {
            text_input: 10,
            audio_input: 20,
            cached_input: 5,
            text_output: 3,
            audio_output: 7,
        };
        snapshot.record("realtime_voice", OPENAI_REALTIME_VOICE_MODEL, counts);
        snapshot.record("goal_formation", "gpt-5-mini", counts);
        snapshot.record(
            "realtime_voice",
            OPENAI_REALTIME_VOICE_MODEL,
            TokenClassCounts {
                text_input: 1,
                ..TokenClassCounts::default()
            },
        );

        assert_eq!(snapshot.models.len(), 2);
        assert_eq!(snapshot.models[0].role, "realtime_voice");
        assert_eq!(snapshot.models[0].calls, 2);
        assert_eq!(snapshot.models[0].counts.text_input, 11);
        assert_eq!(snapshot.models[0].counts.audio_input, 20);
        assert_eq!(snapshot.models[1].role, "goal_formation");
        assert_eq!(snapshot.models[1].calls, 1);
    }
}
