use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionFixture {
    pub tensions: Vec<Tension>,
    pub goals: Vec<Goal>,
}

/// A persistent pressure that names what the system cares about. Tensions back goals and
/// determine arbitration precedence when multiple goals compete.
///
/// ## Arbitration tier
///
/// `arbitration_tier` places this tension in the conflict-resolution hierarchy. A goal's
/// effective tier is the minimum `arbitration_tier` among its parent tensions (defaulting
/// to `u8::MAX` if it has no parent tensions in the fixture). Lower tier wins.
///
/// Covered tiers in the current fixture:
/// - **1** — Safety and project boundaries (`boundary-preservation`)
/// - **4** — Coherence and self-correction (`coherence-maintenance`)
/// - **5** — Continuity preservation (`continuity-preservation`)
/// - **7** — Research curiosity (`research-curiosity`)
///
/// Extension points (not yet covered by any fixture tension):
/// - **2** — Explicit user intent
/// - **3** — Current task completion
/// - **6** — Active experiment mode
/// - **8** — Optional exploration
///
/// Future tensions must be assigned the correct tier when added. A goal with effective
/// tier `u8::MAX` (no parent tensions in the fixture) is a signal that a tension
/// assignment is missing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Tension {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub priority_bias: TensionPriority,
    /// Arbitration precedence tier; lower tier wins conflict resolution. See type doc.
    pub arbitration_tier: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TensionPriority {
    Lowest,
    Low,
    Medium,
    High,
    Highest,
}

impl TensionPriority {
    pub fn score_bonus(self) -> f64 {
        match self {
            Self::Lowest => 0.0,
            Self::Low => 5.0,
            Self::Medium => 10.0,
            Self::High => 15.0,
            Self::Highest => 20.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub tension_ids: Vec<String>,
    pub status: GoalStatus,
    pub scope: GoalScope,
    pub base_priority: u8,
    pub activation_keywords: Vec<String>,
    pub allowed_effects: Vec<AllowedEffect>,
    pub satisfaction_condition_summary: String,
    pub evidence_refs: Vec<String>,
    pub estimated_tokens: usize,
    pub source_reference: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Proposed,
    Accepted,
    Active,
    Blocked,
    Satisfied,
    Cooldown,
    Retired,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalScope {
    Input,
    Session,
    Project,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowedEffect {
    Reflect,
    RetrieveContext,
    ProposeExperiment,
    SurfaceOpenThread,
}

impl fmt::Display for GoalStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Satisfied => "satisfied",
            Self::Cooldown => "cooldown",
            Self::Retired => "retired",
        })
    }
}

impl fmt::Display for GoalScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Input => "input",
            Self::Session => "session",
            Self::Project => "project",
        })
    }
}

impl fmt::Display for AllowedEffect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Reflect => "reflect",
            Self::RetrieveContext => "retrieve-context",
            Self::ProposeExperiment => "propose-experiment",
            Self::SurfaceOpenThread => "surface-open-thread",
        })
    }
}

impl fmt::Display for TensionPriority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Lowest => "lowest",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Highest => "highest",
        })
    }
}
