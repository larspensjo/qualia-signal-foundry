use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Frame,
    Concept,
    Research,
    Plan,
    Idea,
    Design,
    Architecture,
    ExperimentSpec,
    ExperimentReport,
    Decision,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaturityTag {
    Brainstorm,
    Sketch,
    Candidate,
    Accepted,
    Implemented,
    Deprecated,
    Unknown,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStrength {
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DocHit {
    pub path: String,
    pub kind: DocKind,
    pub maturity_tag: MaturityTag,
    pub last_reviewed: Option<String>,
    pub snippet: String,
    pub section_hint: Option<String>,
    pub match_strength: MatchStrength,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DocRead {
    pub path: String,
    pub kind: DocKind,
    pub maturity_tag: MaturityTag,
    pub last_reviewed: Option<String>,
    pub content: String,
    pub is_full: bool,
    pub omitted_sections: Vec<String>,
}
