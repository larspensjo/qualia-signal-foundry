//! Per-model documented max context windows used by live token-budget aging.

pub const DEFAULT_MODEL_MAX_TOKENS: usize = 200_000;

const ENTRIES: &[(&str, usize)] = &[
    ("gpt-5.4-mini", 200_000),
    ("gpt-5.4", 400_000),
    ("gpt-5.5", 400_000),
    ("gpt-5.3-codex", 200_000),
    ("gpt-5.2", 200_000),
];

pub fn model_max_tokens(model_id: &str) -> Option<usize> {
    ENTRIES
        .iter()
        .find(|(id, _)| *id == model_id)
        .map(|(_, tokens)| *tokens)
}

pub fn model_max_tokens_or_default(model_id: &str) -> (usize, bool) {
    match model_max_tokens(model_id) {
        Some(tokens) => (tokens, true),
        None => (DEFAULT_MODEL_MAX_TOKENS, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_model_returns_window() {
        assert_eq!(model_max_tokens("gpt-5.4-mini"), Some(200_000));
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(model_max_tokens("imaginary-model").is_none());
    }

    #[test]
    fn unknown_model_falls_back_to_default_window() {
        assert_eq!(
            model_max_tokens_or_default("imaginary-model"),
            (DEFAULT_MODEL_MAX_TOKENS, false)
        );
    }
}
