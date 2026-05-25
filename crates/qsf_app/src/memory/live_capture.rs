#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveMemoryCandidateKind {
    AssistantName,
    UserName,
    RememberedTopic,
}

impl LiveMemoryCandidateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AssistantName => "assistant-name",
            Self::UserName => "user-name",
            Self::RememberedTopic => "remembered-topic",
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

    if let Some(candidate) = capture_remembered_topic(input) {
        candidates.push(candidate);
    }

    candidates
}

pub fn remember_this_skip_reason(input: &LiveCaptureInput<'_>) -> Option<&'static str> {
    if !explicit_remember_request(input.user_input) {
        return None;
    }

    if input
        .previous_assistant_response
        .map(|response| response.trim().is_empty())
        .unwrap_or(true)
    {
        return Some(
            "explicit remember-this request had no previous assistant response to preserve",
        );
    }

    None
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

fn capture_remembered_topic(input: &LiveCaptureInput<'_>) -> Option<LiveMemoryCandidate> {
    if !explicit_remember_request(input.user_input) {
        return None;
    }

    let previous_assistant_response = input
        .previous_assistant_response
        .map(str::trim)
        .filter(|response| !response.is_empty())?;
    let source_excerpt = bounded_source_excerpt(previous_assistant_response, 180);
    let topic_terms = extract_topic_terms(input.previous_user_input.unwrap_or(input.user_input));
    let topic_label = remembered_topic_label(&topic_terms);
    let mut tags = vec!["remembered_topic".to_string()];
    tags.extend(topic_terms.iter().cloned());

    Some(LiveMemoryCandidate {
        candidate_kind: LiveMemoryCandidateKind::RememberedTopic,
        id_suffix: LiveMemoryCandidateKind::RememberedTopic
            .as_str()
            .to_string(),
        title: format!("Remembered topic: {topic_label}"),
        summary: format!(
            "The user asked the assistant to remember the prior discussion for future conversations. Topic: {topic_label}. Source excerpt: {source_excerpt}"
        ),
        tags,
        importance: 0.8,
        source_turn_index: input.previous_turn_index,
    })
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

fn explicit_remember_request(input: &str) -> bool {
    let normalized = normalize_for_match(input);

    if normalized.contains(" don't remember this ")
        || normalized.contains(" dont remember this ")
        || normalized.contains(" do not remember this ")
        || normalized.contains(" not remember this ")
        || normalized.contains(" i remember this ")
        || normalized.contains(" i should remember this ")
        || normalized.contains(" i want to remember this ")
        || normalized.contains(" i need to remember this ")
        || normalized.contains(" let me remember this ")
    {
        return false;
    }

    normalized.starts_with(" remember this ")
        || normalized.contains(" please remember this ")
        || normalized.contains(" can you remember this ")
        || normalized.contains(" could you remember this ")
        || normalized.contains(" would you remember this ")
        || normalized.contains(" remember that for future ")
        || normalized.starts_with(" keep this in mind ")
        || normalized.contains(" please keep this in mind ")
}

fn extract_topic_terms(input: &str) -> Vec<String> {
    let normalized = normalize_for_match(input);
    let mut terms = Vec::new();

    push_unique_term(&mut terms, &normalized, "volition");
    push_unique_term(&mut terms, &normalized, "system");
    push_unique_topic_tag(&mut terms, &normalized, &["goal", "goals"], "goal");
    push_unique_topic_tag(&mut terms, &normalized, &["goal", "goals"], "goals");
    push_unique_term(&mut terms, &normalized, "simulation");
    push_unique_term(&mut terms, &normalized, "memory");
    push_unique_term(&mut terms, &normalized, "retrieval");
    push_unique_term(&mut terms, &normalized, "context");
    push_unique_term(&mut terms, &normalized, "identity");
    push_unique_term(&mut terms, &normalized, "name");
    push_unique_term(&mut terms, &normalized, "arbitration");
    push_unique_term(&mut terms, &normalized, "preference");

    if terms.contains(&"volition".to_string()) && terms.contains(&"system".to_string()) {
        terms.push("volition_system".to_string());
    }

    terms
}

fn push_unique_term(terms: &mut Vec<String>, normalized: &str, term: &str) {
    if normalized.contains(&format!(" {term} ")) && !terms.iter().any(|existing| existing == term) {
        terms.push(term.to_string());
    }
}

fn push_unique_topic_tag(
    terms: &mut Vec<String>,
    normalized: &str,
    aliases: &[&str],
    canonical: &str,
) {
    if aliases
        .iter()
        .any(|alias| normalized.contains(&format!(" {alias} ")))
        && !terms.iter().any(|existing| existing == canonical)
    {
        terms.push(canonical.to_string());
    }
}

fn remembered_topic_label(topic_terms: &[String]) -> String {
    if topic_terms.is_empty() {
        return "prior discussion".to_string();
    }

    if topic_terms.iter().any(|term| term == "volition")
        && topic_terms.iter().any(|term| term == "system")
    {
        return "volition system".to_string();
    }

    topic_terms
        .iter()
        .filter(|term| term.as_str() != "volition_system")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
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

fn normalize_for_match(input: &str) -> String {
    format!(
        " {} ",
        input
            .split(|character: char| !character.is_alphanumeric())
            .filter(|term| !term.is_empty())
            .map(|term| term.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn bounded_source_excerpt(text: &str, char_cap: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= char_cap {
        return normalized;
    }

    let preview_len = char_cap.saturating_sub(3);
    let mut excerpt = normalized.chars().take(preview_len).collect::<String>();
    excerpt.push_str("...");
    excerpt
}

#[cfg(test)]
mod tests {
    use super::{
        LiveCaptureInput, LiveMemoryCandidateKind, capture_live_memory_candidates,
        explicit_remember_request, remember_this_skip_reason,
    };

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
    fn captures_remembered_topic_from_previous_context() {
        let input = LiveCaptureInput {
            user_input: "Interesting, please remember this for future discussions!",
            assistant_response: "Sure, I will remember it.",
            previous_turn_index: Some(7),
            previous_user_input: Some(
                "Tell me more what you think how a volition system should work.",
            ),
            previous_assistant_response: Some(
                "A good volition system should include needs/drives, goals, arbitration, and continuity.",
            ),
        };

        let candidates = capture_live_memory_candidates(&input);
        let remembered = candidates
            .iter()
            .find(|candidate| candidate.candidate_kind == LiveMemoryCandidateKind::RememberedTopic)
            .expect("expected remembered-topic candidate");

        assert_eq!(remembered.id_suffix, "remembered-topic");
        assert_eq!(remembered.source_turn_index, Some(7));
        assert_eq!(remembered.title, "Remembered topic: volition system");
        assert!(remembered.summary.contains("Topic: volition system."));
        assert!(remembered.summary.contains("Source excerpt:"));
        assert!(remembered.tags.iter().any(|tag| tag == "remembered_topic"));
        assert!(remembered.tags.iter().any(|tag| tag == "volition"));
        assert!(remembered.tags.iter().any(|tag| tag == "system"));
        assert!(remembered.tags.iter().any(|tag| tag == "volition_system"));
    }

    #[test]
    fn skips_remembered_topic_when_previous_assistant_response_is_whitespace() {
        let input = LiveCaptureInput {
            user_input: "Please remember this for future discussions.",
            assistant_response: "Sure.",
            previous_turn_index: Some(3),
            previous_user_input: Some("Tell me more about volition."),
            previous_assistant_response: Some("   "),
        };

        assert_eq!(
            remember_this_skip_reason(&input),
            Some("explicit remember-this request had no previous assistant response to preserve")
        );
        assert!(capture_live_memory_candidates(&input).is_empty());
    }

    #[test]
    fn remembered_topic_tags_goal_aliases_symmetrically() {
        let singular = capture_live_memory_candidates(&LiveCaptureInput {
            user_input: "Please remember this for future discussions.",
            assistant_response: "Sure.",
            previous_turn_index: Some(1),
            previous_user_input: Some("What about a goal arbitration system?"),
            previous_assistant_response: Some("A goal arbitration system should compare drives."),
        });
        let plural = capture_live_memory_candidates(&LiveCaptureInput {
            user_input: "Please remember this for future discussions.",
            assistant_response: "Sure.",
            previous_turn_index: Some(1),
            previous_user_input: Some("What about goals in an arbitration system?"),
            previous_assistant_response: Some("Goals should be compared through arbitration."),
        });

        let singular = singular
            .iter()
            .find(|candidate| candidate.candidate_kind == LiveMemoryCandidateKind::RememberedTopic)
            .unwrap();
        let plural = plural
            .iter()
            .find(|candidate| candidate.candidate_kind == LiveMemoryCandidateKind::RememberedTopic)
            .unwrap();

        assert!(singular.tags.iter().any(|tag| tag == "goal"));
        assert!(singular.tags.iter().any(|tag| tag == "goals"));
        assert!(plural.tags.iter().any(|tag| tag == "goal"));
        assert!(plural.tags.iter().any(|tag| tag == "goals"));
    }

    #[test]
    fn detects_explicit_remember_request() {
        assert!(explicit_remember_request(
            "Interesting, please remember this for future discussions!"
        ));
    }

    #[test]
    fn negated_or_self_directed_remember_this_does_not_trigger_capture() {
        assert!(!explicit_remember_request(
            "Don't remember this - I'm joking."
        ));
        assert!(!explicit_remember_request(
            "I want to remember this for myself later."
        ));
        assert!(!explicit_remember_request(
            "I should remember this for future reference."
        ));
    }

    #[test]
    fn ordinary_memory_word_does_not_trigger_capture() {
        assert!(!explicit_remember_request(
            "How does memory influence goal selection?"
        ));

        let input = LiveCaptureInput {
            user_input: "How does memory influence goal selection?",
            assistant_response: "It changes prioritization.",
            previous_turn_index: Some(2),
            previous_user_input: Some("Tell me more about retrieval."),
            previous_assistant_response: Some("Retrieval should stay relevance-gated."),
        };

        let candidates = capture_live_memory_candidates(&input);
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.candidate_kind
                    != LiveMemoryCandidateKind::RememberedTopic)
        );
    }

    #[test]
    fn remember_request_without_previous_assistant_response_reports_skip_reason() {
        let input = LiveCaptureInput {
            user_input: "Please remember this for future discussions.",
            assistant_response: "Sure.",
            previous_turn_index: Some(3),
            previous_user_input: Some("Tell me more about volition."),
            previous_assistant_response: None,
        };

        assert_eq!(
            remember_this_skip_reason(&input),
            Some("explicit remember-this request had no previous assistant response to preserve")
        );
        assert!(capture_live_memory_candidates(&input).is_empty());
    }
}
