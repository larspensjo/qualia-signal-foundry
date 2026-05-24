//! Conservative character-based token estimator shared by memory construction
//! and live-loop hot-context aging.

pub fn estimated_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}
