use serde::{Deserialize, Serialize};
use std::fmt;

/// A byte span into the original input text. The span is half-open:
/// `start..end`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
}

/// A grounded reference that can cite either a user input span or a goal id.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GroundingRef {
    InputTerm {
        normalized_text: String,
        original_text: String,
        span: TextSpan,
    },
    GoalId {
        goal_id: String,
    },
}

impl fmt::Display for GroundingRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTerm {
                normalized_text,
                original_text,
                span,
            } => write!(
                formatter,
                "input_term:{}[{}..{}]={}",
                normalized_text, span.start, span.end, original_text
            ),
            Self::GoalId { goal_id } => write!(formatter, "goal:{goal_id}"),
        }
    }
}

/// A grounded token from the original input text. It preserves the original
/// text and byte span so later traces can cite a real grounding ref instead of
/// reconstructing from normalized terms alone.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroundedTerm {
    pub normalized_text: String,
    pub original_text: String,
    pub span: TextSpan,
}

impl GroundedTerm {
    pub fn grounding_ref(&self) -> GroundingRef {
        GroundingRef::InputTerm {
            normalized_text: self.normalized_text.clone(),
            original_text: self.original_text.clone(),
            span: self.span.clone(),
        }
    }
}

/// Normalize input text into lowercase word tokens, deduplicated, in order of first
/// appearance. Used by goal selection and candidate proposal matching.
pub fn normalize_terms(input: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();

    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            if !terms.iter().any(|term| term == &current) {
                terms.push(current.clone());
            }
            current.clear();
        }
    }

    if !current.is_empty() && !terms.iter().any(|term| term == &current) {
        terms.push(current);
    }

    terms
}

/// Tokenize the input into grounded terms while preserving the first span for each
/// normalized token. This reuses the same normalization rules as `normalize_terms`.
pub fn grounded_terms_from_text(input: &str) -> Vec<GroundedTerm> {
    let normalized_terms = normalize_terms(input);
    let mut grounded = Vec::new();
    let mut seen = Vec::<String>::new();
    let mut current = String::new();
    let mut start = None;

    let mut finish_term = |start: Option<usize>, end: usize, current: &mut String| {
        let Some(start) = start else {
            current.clear();
            return;
        };
        let normalized_text = current.clone();
        current.clear();
        if normalized_text.is_empty()
            || !normalized_terms.iter().any(|term| term == &normalized_text)
            || seen.iter().any(|term| term == &normalized_text)
        {
            return;
        }
        seen.push(normalized_text.clone());
        grounded.push(GroundedTerm {
            normalized_text,
            original_text: input[start..end].to_string(),
            span: TextSpan { start, end },
        });
    };

    for (index, character) in input.char_indices() {
        if character.is_ascii_alphanumeric() {
            if start.is_none() {
                start = Some(index);
            }
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            finish_term(start.take(), index, &mut current);
        } else {
            start = None;
        }
    }

    if !current.is_empty() {
        finish_term(start.take(), input.len(), &mut current);
    }

    grounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounded_terms_preserve_span_and_normalization() {
        let grounded = grounded_terms_from_text("How can you help me?");
        assert_eq!(grounded[0].normalized_text, "how");
        assert_eq!(grounded[0].original_text, "How");
        assert_eq!(grounded[0].span.start, 0);
        assert_eq!(grounded[0].span.end, 3);
    }

    #[test]
    fn grounded_terms_are_deduplicated_like_normalize_terms() {
        let grounded = grounded_terms_from_text("help help HELP");
        assert_eq!(grounded.len(), 1);
        assert_eq!(grounded[0].normalized_text, "help");
    }
}
