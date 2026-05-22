use axum::{Json, Router, extract::State, routing::get};
use axum_extra::extract::Query;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

const MAX_RESULTS: usize = 20;
const EXCERPT_RADIUS: usize = 96;

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionSearchResponse {
    pub available: bool,
    pub path: String,
    pub session_id: Option<String>,
    pub total: usize,
    pub items: Vec<SessionSearchItem>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionSearchItem {
    pub kind: SessionSearchItemKind,
    pub turn_index: usize,
    pub title: String,
    pub excerpt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSearchItemKind {
    Turn,
    TurnSummary,
    RecalledTurn,
}

#[derive(Debug, Deserialize)]
struct SessionStateDocument {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    turns: Vec<SessionTurn>,
    #[serde(default)]
    summarized_turns: Vec<SessionTurnSummary>,
}

#[derive(Debug, Deserialize)]
struct SessionTurn {
    index: usize,
    #[serde(default)]
    user_input: String,
    #[serde(default)]
    assistant_response: String,
    #[serde(default)]
    recalled_turns: Vec<SessionRecall>,
}

#[derive(Debug, Deserialize)]
struct SessionRecall {
    turn_id: usize,
    #[serde(default)]
    verbatim_text: String,
}

#[derive(Debug, Deserialize)]
struct SessionTurnSummary {
    turn_index: usize,
    #[serde(default)]
    summary: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/session/search", get(search_session))
}

async fn search_session(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<SessionSearchResponse> {
    Json(search_session_state(
        state.session_state_path(),
        query.q.as_deref().unwrap_or_default(),
    ))
}

fn search_session_state(path: &std::path::Path, query: &str) -> SessionSearchResponse {
    let path_text = path.display().to_string();
    if query.trim().is_empty() {
        return SessionSearchResponse {
            available: path.exists(),
            path: path_text,
            session_id: None,
            total: 0,
            items: vec![],
            message: None,
        };
    }

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return SessionSearchResponse {
                available: false,
                path: path_text,
                session_id: None,
                total: 0,
                items: vec![],
                message: Some("session-state.json was not found next to the memory store".into()),
            };
        }
        Err(err) => {
            return SessionSearchResponse {
                available: false,
                path: path_text,
                session_id: None,
                total: 0,
                items: vec![],
                message: Some(format!("failed to read session state: {err}")),
            };
        }
    };

    let parsed: SessionStateDocument = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(err) => {
            return SessionSearchResponse {
                available: false,
                path: path_text,
                session_id: None,
                total: 0,
                items: vec![],
                message: Some(format!("failed to parse session state: {err}")),
            };
        }
    };

    let needle = query.to_lowercase();
    let mut items = Vec::new();
    for summary in &parsed.summarized_turns {
        push_match(
            &mut items,
            SessionSearchItemKind::TurnSummary,
            summary.turn_index,
            "summarized turn",
            &summary.summary,
            &needle,
        );
    }
    for turn in &parsed.turns {
        let text = format!(
            "[User]\n{}\n\n[Assistant]\n{}",
            turn.user_input, turn.assistant_response
        );
        push_match(
            &mut items,
            SessionSearchItemKind::Turn,
            turn.index,
            "session turn",
            &text,
            &needle,
        );
        for recall in &turn.recalled_turns {
            push_match(
                &mut items,
                SessionSearchItemKind::RecalledTurn,
                recall.turn_id,
                "recalled turn",
                &recall.verbatim_text,
                &needle,
            );
        }
    }

    let total = items.len();
    items.truncate(MAX_RESULTS);
    SessionSearchResponse {
        available: true,
        path: path_text,
        session_id: Some(parsed.session_id),
        total,
        items,
        message: None,
    }
}

fn push_match(
    items: &mut Vec<SessionSearchItem>,
    kind: SessionSearchItemKind,
    turn_index: usize,
    label: &str,
    text: &str,
    needle: &str,
) {
    if !text.to_lowercase().contains(needle) {
        return;
    }
    items.push(SessionSearchItem {
        kind,
        turn_index,
        title: format!("{label} {turn_index}"),
        excerpt: excerpt(text, needle),
    });
}

fn excerpt(text: &str, needle: &str) -> String {
    let lower = text.to_lowercase();
    let Some(byte_index) = lower.find(needle) else {
        return text.chars().take(EXCERPT_RADIUS * 2).collect();
    };

    let char_index = text[..byte_index].chars().count();
    let start = char_index.saturating_sub(EXCERPT_RADIUS);
    let end = char_index + needle.chars().count() + EXCERPT_RADIUS;
    let excerpt = text
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect::<String>();

    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if text.chars().count() > end {
        "..."
    } else {
        ""
    };
    format!("{prefix}{}{suffix}", excerpt.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_summarized_turn_text() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("session-state.json");
        std::fs::write(
            &path,
            r#"{
                "session_id": "s-test",
                "turns": [],
                "summarized_turns": [
                    {
                        "turn_index": 0,
                        "summary": "The user asked the assistant to remember that its name is Ari."
                    }
                ]
            }"#,
        )
        .unwrap();

        let result = search_session_state(&path, "Ari");

        assert!(result.available);
        assert_eq!(result.session_id.as_deref(), Some("s-test"));
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].turn_index, 0);
        assert!(result.items[0].excerpt.contains("Ari"));
    }

    #[test]
    fn missing_session_state_is_reported_without_failing_memory_health() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("missing-session-state.json");

        let result = search_session_state(&path, "Ari");

        assert!(!result.available);
        assert_eq!(result.total, 0);
        assert!(result.message.unwrap().contains("not found"));
    }
}
