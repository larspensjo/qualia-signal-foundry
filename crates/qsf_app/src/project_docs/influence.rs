//! Best-effort overlap check used to mark whether a returned project-doc
//! excerpt influenced the final assistant reply.

use std::collections::HashSet;

const MIN_NGRAM_SIZE: usize = 4;
const MIN_WORD_LEN: usize = 3;

fn qualifying_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|word| word.trim_matches('\'').to_ascii_lowercase())
        .filter(|word| word.len() >= MIN_WORD_LEN)
        .collect()
}

/// Returns true when `reply` and `excerpt` share a contiguous run of at
/// least `MIN_NGRAM_SIZE` qualifying words.
pub fn reply_overlaps_excerpt(reply: &str, excerpt: &str) -> bool {
    let excerpt_words = qualifying_words(excerpt);
    let reply_words = qualifying_words(reply);
    if excerpt_words.len() < MIN_NGRAM_SIZE || reply_words.len() < MIN_NGRAM_SIZE {
        return false;
    }

    let reply_ngrams: HashSet<String> = reply_words
        .windows(MIN_NGRAM_SIZE)
        .map(|window| window.join(" "))
        .collect();
    excerpt_words
        .windows(MIN_NGRAM_SIZE)
        .any(|window| reply_ngrams.contains(&window.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::reply_overlaps_excerpt;

    #[test]
    fn overlapping_reply_is_marked_influenced() {
        let excerpt = "The project's accepted framing says autonomy is deferred.";
        let reply = "As the project's accepted framing says, that part is deferred.";

        assert!(reply_overlaps_excerpt(reply, excerpt));
    }

    #[test]
    fn unrelated_reply_is_not_influenced() {
        let excerpt = "The project's accepted framing says autonomy is deferred.";
        let reply = "The capital of France is Paris.";

        assert!(!reply_overlaps_excerpt(reply, excerpt));
    }

    #[test]
    fn short_excerpt_below_ngram_size_is_not_influenced() {
        assert!(!reply_overlaps_excerpt("anything at all here", "tiny note"));
    }
}
