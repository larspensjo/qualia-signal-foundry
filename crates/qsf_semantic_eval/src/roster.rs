use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use qsf_volition::{Goal, VolitionFixture, VolitionState};

pub const ROSTER_SNAPSHOT_SCHEMA_VERSION: u16 = 1;
pub const SAMPLE_ROSTER_SNAPSHOT_VERSION: &str = "realtime-seed-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenGoalRef {
    pub goal_ref: String,
    pub title: String,
    pub summary: String,
    pub tension_summaries: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RosterSnapshot {
    pub schema_version: u16,
    pub roster_snapshot_version: String,
    pub fixture: VolitionFixture,
    pub default_state: VolitionState,
    pub fixture_hash: String,
    pub goals: Vec<FrozenGoalRef>,
}

impl RosterSnapshot {
    pub fn from_realtime_seed() -> Self {
        let fixture = qsf_volition::realtime_seed_fixture();
        Self::from_fixture(SAMPLE_ROSTER_SNAPSHOT_VERSION, fixture)
    }

    pub fn from_fixture(
        roster_snapshot_version: impl Into<String>,
        fixture: VolitionFixture,
    ) -> Self {
        let roster_snapshot_version = roster_snapshot_version.into();
        let goals = fixture
            .goals
            .iter()
            .map(|goal| frozen_goal_ref(goal, &fixture, &roster_snapshot_version))
            .collect();
        Self {
            schema_version: ROSTER_SNAPSHOT_SCHEMA_VERSION,
            default_state: VolitionState::from_fixture(&fixture),
            fixture_hash: fixture_hash(&fixture).expect("VolitionFixture must serialize"),
            fixture,
            roster_snapshot_version,
            goals,
        }
    }

    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|error| {
            format!("could not read roster snapshot {}: {error}", path.display())
        })?;
        let snapshot: Self = serde_json::from_str(&input)
            .map_err(|error| format!("invalid roster snapshot {}: {error}", path.display()))?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn write_json_path(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "could not create roster directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("could not serialize roster snapshot: {error}"))?;
        fs::write(path, format!("{json}\n")).map_err(|error| {
            format!(
                "could not write roster snapshot {}: {error}",
                path.display()
            )
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ROSTER_SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "roster snapshot uses schema_version {}; supported version is {}",
                self.schema_version, ROSTER_SNAPSHOT_SCHEMA_VERSION
            ));
        }
        let actual_hash = fixture_hash(&self.fixture)?;
        if self.fixture_hash != actual_hash {
            return Err(format!(
                "roster fixture hash mismatch: stored {}, computed {}",
                self.fixture_hash, actual_hash
            ));
        }
        if self.default_state != VolitionState::from_fixture(&self.fixture) {
            return Err(
                "roster default_state does not match VolitionState::from_fixture".to_string(),
            );
        }
        let expected_refs: Vec<_> = self
            .fixture
            .goals
            .iter()
            .map(|goal| frozen_goal_ref(goal, &self.fixture, &self.roster_snapshot_version))
            .collect();
        if self.goals != expected_refs {
            return Err(
                "roster goal references do not match the frozen fixture descriptions".to_string(),
            );
        }
        Ok(())
    }

    pub fn goal_for_ref(&self, goal_ref: &str) -> Option<&Goal> {
        let index = self
            .goals
            .iter()
            .position(|goal| goal.goal_ref == goal_ref)?;
        self.fixture.goals.get(index)
    }

    pub fn assert_matches_current_realtime_seed(&self) -> Result<(), String> {
        let current = Self::from_realtime_seed();
        if self != &current {
            return Err(format!(
                "frozen roster {} drifted from qsf_volition::realtime_seed_fixture(); re-version the snapshot deliberately (frozen hash {}, current hash {})",
                self.roster_snapshot_version, self.fixture_hash, current.fixture_hash
            ));
        }
        Ok(())
    }
}

pub fn fixture_hash(fixture: &VolitionFixture) -> Result<String, String> {
    let bytes = serde_json::to_vec(fixture)
        .map_err(|error| format!("could not serialize fixture for content hash: {error}"))?;
    Ok(format!(
        "sha256:{}",
        hex_digest(Sha256::digest(bytes).as_ref())
    ))
}

pub fn goal_ref_description_key(
    goal: &Goal,
    fixture: &VolitionFixture,
    roster_snapshot_version: &str,
) -> String {
    frozen_goal_ref(goal, fixture, roster_snapshot_version).goal_ref
}

fn frozen_goal_ref(
    goal: &Goal,
    fixture: &VolitionFixture,
    roster_snapshot_version: &str,
) -> FrozenGoalRef {
    let tension_summaries = goal
        .tension_ids
        .iter()
        .filter_map(|id| fixture.tensions.iter().find(|tension| tension.id == *id))
        .map(|tension| tension.summary.clone())
        .collect::<Vec<_>>();
    let description = serde_json::to_vec(&(
        roster_snapshot_version,
        goal.title.as_str(),
        goal.summary.as_str(),
        tension_summaries.as_slice(),
    ))
    .expect("goal description tuple must serialize");
    FrozenGoalRef {
        goal_ref: format!(
            "sha256:{}",
            hex_digest(Sha256::digest(description).as_ref())
        ),
        title: goal.title.clone(),
        summary: goal.summary.clone(),
        tension_summaries,
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
