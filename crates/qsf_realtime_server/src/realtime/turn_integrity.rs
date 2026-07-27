pub use qsf_diagnostics::TurnPhase;

const CONTINUATION_NOISE_ALLOW_LIST: &[&str] = &["cheers", "thanks", "thank you"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptDisposition {
    StartTurn,
    IgnoreAsNoise,
    Interrupt,
}

pub(crate) fn classify_final_transcript(
    phase: TurnPhase,
    transcript: &str,
) -> TranscriptDisposition {
    match phase {
        TurnPhase::Idle => TranscriptDisposition::StartTurn,
        TurnPhase::AwaitingResponse | TurnPhase::ToolLoop => {
            let normalized = normalize_final_transcript(transcript);
            if normalized.is_empty() || CONTINUATION_NOISE_ALLOW_LIST.contains(&normalized.as_str())
            {
                TranscriptDisposition::IgnoreAsNoise
            } else {
                TranscriptDisposition::Interrupt
            }
        }
    }
}

fn normalize_final_transcript(transcript: &str) -> String {
    transcript
        .trim()
        .to_lowercase()
        .trim_end_matches(|character: char| character.is_ascii_punctuation())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_trims_and_strips_trailing_punctuation() {
        assert_eq!(normalize_final_transcript("  Thank you.  "), "thank you");
    }

    #[test]
    fn idle_short_transcript_starts_new_turn() {
        assert_eq!(
            classify_final_transcript(TurnPhase::Idle, "thanks"),
            TranscriptDisposition::StartTurn
        );
    }

    #[test]
    fn in_flight_allow_list_hits_and_misses() {
        assert_eq!(
            classify_final_transcript(TurnPhase::AwaitingResponse, "Cheers"),
            TranscriptDisposition::IgnoreAsNoise
        );
        assert_eq!(
            classify_final_transcript(TurnPhase::ToolLoop, "Thanks!"),
            TranscriptDisposition::IgnoreAsNoise
        );
        assert_eq!(
            classify_final_transcript(TurnPhase::AwaitingResponse, "Thank you."),
            TranscriptDisposition::IgnoreAsNoise
        );
        assert_eq!(
            classify_final_transcript(TurnPhase::ToolLoop, "stop"),
            TranscriptDisposition::Interrupt
        );
        assert_eq!(
            classify_final_transcript(TurnPhase::AwaitingResponse, "wait"),
            TranscriptDisposition::Interrupt
        );
    }

    #[test]
    fn in_flight_empty_transcript_is_ignored() {
        assert_eq!(
            classify_final_transcript(TurnPhase::AwaitingResponse, "   "),
            TranscriptDisposition::IgnoreAsNoise
        );
    }

    #[test]
    fn turn_phase_serde_uses_snake_case() {
        let encoded = serde_json::to_value(TurnPhase::AwaitingResponse).unwrap();
        assert_eq!(encoded, serde_json::json!("awaiting_response"));

        let decoded: TurnPhase = serde_json::from_value(serde_json::json!("tool_loop")).unwrap();
        assert_eq!(decoded, TurnPhase::ToolLoop);
    }
}
