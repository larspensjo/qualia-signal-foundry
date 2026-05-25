#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveMemoryCandidateKind {
    AssistantName,
    UserName,
}

impl LiveMemoryCandidateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AssistantName => "assistant-name",
            Self::UserName => "user-name",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveCaptureInput<'a> {
    pub user_input: &'a str,
    pub assistant_response: &'a str,
    pub previous_turn_index: Option<usize>,
    pub previous_user_input: Option<&'a str>,
    pub previous_assistant_response: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveMemoryCandidate {
    pub candidate_kind: LiveMemoryCandidateKind,
    pub id_suffix: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub importance: f64,
    pub source_turn_index: Option<usize>,
}

pub fn capture_live_memory_candidates(input: &LiveCaptureInput<'_>) -> Vec<LiveMemoryCandidate> {
    let mut candidates = Vec::new();

    if let Some(candidate) =
        capture_assistant_name_assignment(input.user_input, input.assistant_response)
    {
        candidates.push(candidate);
    }

    if let Some(candidate) = capture_user_name_statement(input.user_input) {
        candidates.push(candidate);
    }

    candidates
}

fn capture_assistant_name_assignment(
    user_input: &str,
    assistant_response: &str,
) -> Option<LiveMemoryCandidate> {
    let name = extract_assistant_name_assignment(user_input)?;
    if !contains_case_insensitive(assistant_response, &name) {
        return None;
    }

    Some(LiveMemoryCandidate {
        candidate_kind: LiveMemoryCandidateKind::AssistantName,
        id_suffix: LiveMemoryCandidateKind::AssistantName.as_str().to_string(),
        title: format!("Assistant name: {name}"),
        summary: format!(
            "The user asked the assistant to use the name {name}, and the assistant accepted that name."
        ),
        tags: vec![
            "assistant_identity".to_string(),
            "profile".to_string(),
            "name".to_string(),
        ],
        importance: 0.9,
        source_turn_index: None,
    })
}

fn capture_user_name_statement(user_input: &str) -> Option<LiveMemoryCandidate> {
    if let Some(name) = extract_standalone_user_name(user_input, &["my name is "]) {
        return Some(user_name_candidate(name));
    }

    if let Some(name) = extract_standalone_user_name(user_input, &["call me ", "please call me "]) {
        return Some(user_name_candidate(name));
    }

    if let Some(name) = extract_standalone_user_name(user_input, &["i am "]) {
        return Some(user_name_candidate(name));
    }

    None
}

fn user_name_candidate(name: String) -> LiveMemoryCandidate {
    LiveMemoryCandidate {
        candidate_kind: LiveMemoryCandidateKind::UserName,
        id_suffix: LiveMemoryCandidateKind::UserName.as_str().to_string(),
        title: format!("User name: {name}"),
        summary: format!("The user said their name is {name}."),
        tags: vec![
            "user_identity".to_string(),
            "profile".to_string(),
            "name".to_string(),
        ],
        importance: 0.9,
        source_turn_index: None,
    }
}

fn extract_assistant_name_assignment(input: &str) -> Option<String> {
    const PATTERNS: [&str; 5] = [
        "use the name ",
        "use name ",
        "call you ",
        "call yourself ",
        "your name is ",
    ];

    let lower = input.to_ascii_lowercase();
    PATTERNS.iter().find_map(|pattern| {
        lower
            .find(pattern)
            .and_then(|index| first_name_token(&input[index + pattern.len()..], false))
    })
}

fn extract_standalone_user_name(input: &str, prefixes: &[&str]) -> Option<String> {
    let trimmed = input.trim_start_matches(|c: char| c.is_whitespace() || c == '"' || c == '\'');
    let lower = trimmed.to_ascii_lowercase();
    prefixes.iter().find_map(|prefix| {
        lower
            .strip_prefix(prefix)
            .and_then(|_| first_name_token(&trimmed[prefix.len()..], true))
            .filter(|name| starts_with_uppercase_ascii(name))
    })
}

fn first_name_token(raw: &str, require_short_tail: bool) -> Option<String> {
    let trimmed = raw.trim_start_matches(|c: char| c.is_whitespace() || c == '"' || c == '\'');
    let token = trimmed.split_whitespace().next()?.trim_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'' | '(' | ')' | '[' | ']'
        )
    });
    if token.is_empty() || !is_name_like_token(token) {
        return None;
    }

    if require_short_tail {
        let tail = trimmed
            .split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ");
        if !tail.trim().is_empty() {
            return None;
        }
    }

    Some(token.to_string())
}

fn is_name_like_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    const STOPWORDS: [&str; 16] = [
        "assistant",
        "bot",
        "call",
        "friend",
        "human",
        "i",
        "me",
        "my",
        "name",
        "profile",
        "robot",
        "sir",
        "there",
        "user",
        "you",
        "your",
    ];

    if STOPWORDS.contains(&lower.as_str()) {
        return false;
    }

    let mut chars = token.chars();
    let first = match chars.next() {
        Some(first) => first,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    token.len() >= 2
        && token.len() <= 32
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '\''))
}

fn starts_with_uppercase_ascii(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|character| character.is_ascii_uppercase())
        .unwrap_or(false)
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{LiveCaptureInput, LiveMemoryCandidateKind, capture_live_memory_candidates};

    #[test]
    fn captures_assistant_name_assignment() {
        let input = LiveCaptureInput {
            user_input: "I want you to use the name Ari.",
            assistant_response: "Absolutely - you can call me Ari.",
            previous_turn_index: None,
            previous_user_input: None,
            previous_assistant_response: None,
        };

        let candidates = capture_live_memory_candidates(&input);

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].candidate_kind,
            LiveMemoryCandidateKind::AssistantName
        );
        assert_eq!(candidates[0].id_suffix, "assistant-name");
        assert_eq!(candidates[0].title, "Assistant name: Ari");
        assert!(
            candidates[0]
                .tags
                .iter()
                .any(|tag| tag == "assistant_identity")
        );
    }

    #[test]
    fn captures_user_name_statement() {
        let input = LiveCaptureInput {
            user_input: "My name is Lars.",
            assistant_response: "Noted.",
            previous_turn_index: Some(7),
            previous_user_input: Some("Tell me more about volition."),
            previous_assistant_response: Some("A good volition system should include arbitration."),
        };

        let candidates = capture_live_memory_candidates(&input);

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].candidate_kind,
            LiveMemoryCandidateKind::UserName
        );
        assert_eq!(candidates[0].id_suffix, "user-name");
        assert_eq!(candidates[0].title, "User name: Lars");
        assert!(candidates[0].tags.iter().any(|tag| tag == "user_identity"));
    }

    #[test]
    fn user_name_and_assistant_name_have_distinct_tags_and_ids() {
        let assistant = capture_live_memory_candidates(&LiveCaptureInput {
            user_input: "I want you to use the name Ari.",
            assistant_response: "Absolutely - you can call me Ari.",
            previous_turn_index: None,
            previous_user_input: None,
            previous_assistant_response: None,
        });
        let user = capture_live_memory_candidates(&LiveCaptureInput {
            user_input: "My name is Lars.",
            assistant_response: "Noted.",
            previous_turn_index: None,
            previous_user_input: None,
            previous_assistant_response: None,
        });

        assert_eq!(assistant[0].id_suffix, "assistant-name");
        assert_eq!(user[0].id_suffix, "user-name");
        assert_ne!(assistant[0].tags, user[0].tags);
    }

    #[test]
    fn does_not_capture_common_i_am_state_as_user_name() {
        let input = LiveCaptureInput {
            user_input: "I am tired.",
            assistant_response: "Understood.",
            previous_turn_index: None,
            previous_user_input: None,
            previous_assistant_response: None,
        };

        let candidates = capture_live_memory_candidates(&input);

        assert!(candidates.is_empty());
    }

    #[test]
    fn does_not_capture_callback_request_as_user_name() {
        let input = LiveCaptureInput {
            user_input: "Please call me later when you have time.",
            assistant_response: "Understood.",
            previous_turn_index: None,
            previous_user_input: None,
            previous_assistant_response: None,
        };

        let candidates = capture_live_memory_candidates(&input);

        assert!(candidates.is_empty());
    }

    #[test]
    fn does_not_capture_embedded_my_name_is_phrase() {
        let input = LiveCaptureInput {
            user_input: "I think my name is unique enough.",
            assistant_response: "Understood.",
            previous_turn_index: None,
            previous_user_input: None,
            previous_assistant_response: None,
        };

        let candidates = capture_live_memory_candidates(&input);

        assert!(candidates.is_empty());
    }
}
