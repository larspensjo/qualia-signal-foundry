use std::fmt;

use serde::{Deserialize, Serialize};

/// Minimum match strength a selection needs before it may win arbitration: one Normal
/// keyword qualifies; Weak keywords qualify only in combination (e.g. 4 x Weak).
/// Fixture-level and global by design; per-tier thresholds are deferred until live
/// evidence demands them (DecisionLog 2026-07-04).
pub const DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD: u32 = 4;

fn default_arbitration_qualification_threshold() -> u32 {
    DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionFixture {
    pub tensions: Vec<Tension>,
    pub goals: Vec<Goal>,
    #[serde(default = "default_arbitration_qualification_threshold")]
    pub arbitration_qualification_threshold: u32,
}

/// Coarse activation-keyword weight class. Coarse on purpose: consistent curation beats
/// numeric precision at this goal-set size, and the persona stays data-only
/// (DecisionLog 2026-07-04, weighted goal activation).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeywordWeightClass {
    Weak,
    #[default]
    Normal,
    Strong,
}

impl KeywordWeightClass {
    pub fn weight(self) -> u32 {
        match self {
            Self::Weak => 1,
            Self::Normal => 4,
            Self::Strong => 8,
        }
    }
}

/// One activation keyword with its curated weight class. Serializes in the weighted form;
/// deserializes from either the weighted form or a legacy plain string (default Normal) so
/// pre-weight continuity snapshots, reviewed seeds, and live-formed goals still load.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivationKeyword {
    pub term: String,
    pub weight_class: KeywordWeightClass,
}

impl ActivationKeyword {
    pub fn new(term: impl Into<String>, weight_class: KeywordWeightClass) -> Self {
        Self {
            term: term.into(),
            weight_class,
        }
    }
    pub fn weak(term: impl Into<String>) -> Self {
        Self::new(term, KeywordWeightClass::Weak)
    }
    pub fn normal(term: impl Into<String>) -> Self {
        Self::new(term, KeywordWeightClass::Normal)
    }
    pub fn strong(term: impl Into<String>) -> Self {
        Self::new(term, KeywordWeightClass::Strong)
    }
    pub fn weight(&self) -> u32 {
        self.weight_class.weight()
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ActivationKeywordCompat {
    Weighted {
        term: String,
        weight_class: KeywordWeightClass,
    },
    Legacy(String),
}

impl<'de> Deserialize<'de> for ActivationKeyword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match ActivationKeywordCompat::deserialize(deserializer)? {
            ActivationKeywordCompat::Weighted { term, weight_class } => Self { term, weight_class },
            ActivationKeywordCompat::Legacy(term) => Self::normal(term),
        })
    }
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
/// Tiers are assigned per fixture, not fixed globally. Tiers `1..=PROTECTED_TIER_FLOOR`
/// (see `arbitration::PROTECTED_TIER_FLOOR`) are the protected floor: goals at those tiers
/// are immune to mode bias and to idle-lifecycle retirement. Tiers above the floor form the
/// biasable band, where `Mode` can reorder goals relative to each other. Lower number = higher
/// precedence.
///
/// The two shipped fixtures (`crate::fixture`) illustrate different tier maps:
/// - `static_fixture()` (dev-assistant): `boundary-preservation` (1),
///   `coherence-maintenance` (4), `continuity-preservation` (5), `research-curiosity` (7).
/// - `realtime_seed_fixture()` (curiosity-observer persona): `person-respect` (1),
///   `epistemic-integrity` (2), `present-person-priority` (3), `knowledge-stewardship` (4),
///   `person-curiosity` / `ai-trajectory-concern` (5), `world-curiosity` (6).
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
    /// Mode-bias delta applied to this tension's effective tier under `Mode::Focused`.
    /// Positive demotes (higher tier), negative promotes (lower tier), 0 is neutral.
    /// Must be 0 for protected tiers (≤ `PROTECTED_TIER_FLOOR`), which are bias-immune in code.
    pub focused_bias: i8,
    /// Mode-bias delta applied under `Mode::Exploratory`. Same sign convention as `focused_bias`.
    pub exploratory_bias: i8,
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
    pub activation_keywords: Vec<ActivationKeyword>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_keyword_deserializes_from_legacy_plain_string_as_normal() {
        let keyword: ActivationKeyword = serde_json::from_str("\"economy\"").unwrap();
        assert_eq!(keyword.term, "economy");
        assert_eq!(keyword.weight_class, KeywordWeightClass::Normal);
    }

    #[test]
    fn activation_keyword_roundtrips_weighted_form() {
        let original = ActivationKeyword::strong("automation");
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, r#"{"term":"automation","weight_class":"strong"}"#);
        let reparsed: ActivationKeyword = serde_json::from_str(&json).unwrap();
        assert_eq!(reparsed, original);
    }

    #[test]
    fn weight_class_values_are_one_four_eight() {
        assert_eq!(KeywordWeightClass::Weak.weight(), 1);
        assert_eq!(KeywordWeightClass::Normal.weight(), 4);
        assert_eq!(KeywordWeightClass::Strong.weight(), 8);
    }
}
