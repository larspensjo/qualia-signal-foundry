use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use qsf_memory::{MemoryStore, MemoryStoreContents};
use qsf_session::{CONTINUITY_MANIFEST_SCHEMA_VERSION, ContinuityManifest};
use qsf_volition::{VolitionContinuitySnapshot, persist_volition_continuity_snapshot};
use serde_json::Value;
use time::OffsetDateTime;

const SEED_ROOT: &str = "seed";
const MEMORY_TEMPLATE_FILE: &str = "memory-store.seed.json";
const VOLITION_TEMPLATE_FILE: &str = "volition-state.json";
const CONTINUITY_MANIFEST_FILE: &str = "continuity-manifest.json";

#[derive(Clone, Debug)]
pub struct SeedTemplates {
    pub memory: Value,
    pub volition: VolitionContinuitySnapshot,
    pub manifest: ContinuityManifest,
}

#[derive(Clone, Debug)]
pub struct RenderedSeedBundle {
    pub memory_store: MemoryStoreContents,
    pub volition_state: VolitionContinuitySnapshot,
    pub continuity_manifest: ContinuityManifest,
}

pub fn seed_fixture_dir() -> anyhow::Result<PathBuf> {
    Ok(super::fixture_root()?.join(SEED_ROOT))
}

pub fn load_seed_templates() -> anyhow::Result<SeedTemplates> {
    let root = seed_fixture_dir()?;
    let read = |name: &str| -> anyhow::Result<Vec<u8>> {
        let path = root.join(name);
        fs::read(&path).map_err(|error| {
            anyhow::anyhow!("failed to read seed fixture {}: {error}", path.display())
        })
    };
    let memory = serde_json::from_slice(&read(MEMORY_TEMPLATE_FILE)?)?;
    let volition = serde_json::from_slice(&read(VOLITION_TEMPLATE_FILE)?)?;
    let manifest = parse_manifest_template(&read(CONTINUITY_MANIFEST_FILE)?)?;
    Ok(SeedTemplates {
        memory,
        volition,
        manifest,
    })
}

/// Pure rendering of the checked-in relative-time seed templates. Callers supply `now` so tests
/// and offline preparation never depend on the ambient clock. The probe currently always creates
/// `SessionIdMode::Default`, so both the rewritten snapshot id and materialization target are
/// deliberately coupled to `qsf_session::DEFAULT_SESSION_ID`.
pub fn render_seed_bundle(
    templates: &SeedTemplates,
    now: OffsetDateTime,
) -> anyhow::Result<RenderedSeedBundle> {
    let mut memory = templates.memory.clone();
    let records = memory
        .get_mut("records")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("seed memory template requires records"))?;
    for record in records {
        let object = record
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("seed memory record must be an object"))?;
        let created_days_ago = take_days_ago(object, "created_days_ago")?;
        object.insert(
            "created_at".to_string(),
            Value::String(rfc3339(now - time::Duration::days(created_days_ago))?),
        );
        if object.contains_key("last_reinforced_days_ago") {
            let days = take_days_ago(object, "last_reinforced_days_ago")?;
            object.insert(
                "last_reinforced_at".to_string(),
                Value::String(rfc3339(now - time::Duration::days(days))?),
            );
        }
    }
    let associations = memory
        .get_mut("associations")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("seed memory template requires associations"))?;
    for association in associations {
        let object = association
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("seed memory association must be an object"))?;
        let reinforced_days_ago = take_days_ago(object, "last_reinforced_days_ago")?;
        object.insert(
            "last_reinforced_at".to_string(),
            Value::String(rfc3339(now - time::Duration::days(reinforced_days_ago))?),
        );
    }
    let memory_contents: MemoryStoreContents = serde_json::from_value(memory)?;

    let mut volition = templates.volition.clone();
    volition.recorded_at = rfc3339(now)?;
    volition.qsf_session_id = qsf_session::DEFAULT_SESSION_ID.to_string();
    Ok(RenderedSeedBundle {
        memory_store: memory_contents,
        volition_state: volition,
        continuity_manifest: templates.manifest.clone(),
    })
}

pub fn materialize_seed_bundle(destination: &Path, now: OffsetDateTime) -> anyhow::Result<()> {
    let rendered = render_seed_bundle(&load_seed_templates()?, now)?;
    let target = destination
        .join("continuity")
        .join(qsf_session::DEFAULT_SESSION_ID);
    let memory_path = target.join("memory-store.json");
    let mut memory = MemoryStore::load_or_empty(&memory_path).with_context(|| {
        format!(
            "failed to prepare seed memory store `{}`",
            memory_path.display()
        )
    })?;
    *memory.contents_mut() = rendered.memory_store;
    memory.persist().with_context(|| {
        format!(
            "failed to materialize seed memory store `{}`",
            memory_path.display()
        )
    })?;
    let volition_path = target.join("volition-state.json");
    persist_volition_continuity_snapshot(&rendered.volition_state, &volition_path).with_context(
        || {
            format!(
                "failed to materialize seed volition snapshot `{}`",
                volition_path.display()
            )
        },
    )?;
    let manifest_path = target.join(CONTINUITY_MANIFEST_FILE);
    rendered
        .continuity_manifest
        .persist(&manifest_path)
        .with_context(|| {
            format!(
                "failed to materialize seed manifest `{}`",
                manifest_path.display()
            )
        })
}

fn parse_manifest_template(bytes: &[u8]) -> anyhow::Result<ContinuityManifest> {
    let manifest: ContinuityManifest = serde_json::from_slice(bytes)?;
    if manifest.schema_version != CONTINUITY_MANIFEST_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported seed continuity manifest schema_version: found {} expected {}",
            manifest.schema_version,
            CONTINUITY_MANIFEST_SCHEMA_VERSION
        );
    }
    Ok(manifest)
}

fn take_days_ago(object: &mut serde_json::Map<String, Value>, field: &str) -> anyhow::Result<i64> {
    let days = object
        .remove(field)
        .and_then(|value| value.as_i64())
        .ok_or_else(|| anyhow::anyhow!("seed memory record requires integer {field}"))?;
    if days < 0 {
        anyhow::bail!("seed memory record {field} must not be negative");
    }
    Ok(days)
}

fn rfc3339(value: OffsetDateTime) -> anyhow::Result<String> {
    Ok(value.format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use qsf_memory::{RetrievalRequest, RetrievalStrategy, retrieve_memories};
    use time::macros::datetime;

    use super::*;

    #[test]
    fn materialized_memory_uses_relative_ages_and_retrieves_the_procrastination_record() {
        let templates = load_seed_templates().expect("templates");
        let now = OffsetDateTime::now_utc();
        let rendered = render_seed_bundle(&templates, now).expect("render");
        let contents = rendered.memory_store;
        let raw_records = templates.memory["records"]
            .as_array()
            .expect("template records");
        for (raw, record) in raw_records.iter().zip(&contents.records) {
            let reference = record.last_reinforced_at.unwrap_or(record.created_at);
            let expected = raw
                .get("last_reinforced_days_ago")
                .or_else(|| raw.get("created_days_ago"))
                .and_then(Value::as_i64)
                .expect("offset");
            assert_eq!((now - reference).whole_days(), expected);
        }
        let result = retrieve_memories(&RetrievalRequest::new(
            &contents.records,
            &contents.associations,
            "Can you remember that thesis you had about me putting things off?",
            RetrievalStrategy::AssociationWeighted,
            2,
            now,
        ))
        .expect("retrieve");
        let expected_ids = [
            "invented-procrastination-pattern".to_string(),
            "invented-logistics-transition-context".to_string(),
        ];
        assert_eq!(
            qsf_memory::retrieved_memory_ids(&result.selected),
            expected_ids
        );
        assert!(result.selected[1].score.association > 0.0);

        let future_evaluation_time = now + time::Duration::days(3_650);
        let future = render_seed_bundle(&templates, future_evaluation_time)
            .expect("future render")
            .memory_store;
        let future_result = retrieve_memories(&RetrievalRequest::new(
            &future.records,
            &future.associations,
            "Can you remember that thesis you had about me putting things off?",
            RetrievalStrategy::AssociationWeighted,
            2,
            future_evaluation_time,
        ))
        .expect("future retrieve");
        assert_eq!(
            qsf_memory::retrieved_memory_ids(&future_result.selected),
            expected_ids
        );
    }

    #[test]
    fn seed_records_pin_provenance_and_trust_without_decay_overrides() {
        let templates = load_seed_templates().expect("templates");
        for record in templates.memory["records"].as_array().expect("records") {
            assert!(record.get("provenance").is_some());
            assert!(record.get("trust_tier").is_some());
            assert!(record.get("time_sensitive_decay_half_life_days").is_none());
        }
    }

    #[tokio::test]
    async fn snapshots_restore_when_compatible_and_degrade_to_a_note_when_not() {
        use crate::state::{AppState, SessionIdMode};
        use qsf_volition::{VolitionContinuitySnapshot, realtime_seed_fixture};

        let now = datetime!(2030-01-15 12:00 UTC);
        let compatible_dir = tempfile::tempdir().expect("compatible dir");
        materialize_seed_bundle(compatible_dir.path(), now).expect("materialize");
        let snapshot_path = compatible_dir
            .path()
            .join("continuity/default/volition-state.json");
        let snapshot =
            VolitionContinuitySnapshot::load_or_upgrade(&snapshot_path).expect("snapshot loads");
        let fixture = realtime_seed_fixture();
        assert!(crate::realtime::volition::snapshot_is_fixture_compatible(
            &snapshot.state,
            &fixture
        ));
        assert_eq!(
            snapshot.inspection,
            qsf_volition::build_state_inspection(&snapshot.state, &fixture)
        );
        let state = AppState::new(
            "test-key",
            "http://127.0.0.1:9",
            compatible_dir.path(),
            SessionIdMode::Default,
        )
        .expect("state");
        let allocation = state.create_session().await.expect("session");
        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        assert_eq!(runtime.lock().await.volition.state.tick, 42);

        for malformed in [false, true] {
            let dir = tempfile::tempdir().expect("bad dir");
            materialize_seed_bundle(dir.path(), now).expect("materialize");
            let path = dir.path().join("continuity/default/volition-state.json");
            if malformed {
                fs::write(&path, "{").expect("malformed snapshot");
            } else {
                let mut value: Value =
                    serde_json::from_slice(&fs::read(&path).expect("snapshot bytes"))
                        .expect("snapshot value");
                value["state"]["goals"]
                    .as_object_mut()
                    .expect("goals")
                    .remove("serve-the-present-person");
                fs::write(
                    &path,
                    serde_json::to_vec(&value).expect("serialize incompatible"),
                )
                .expect("incompatible snapshot");
            }
            let state = AppState::new(
                "test-key",
                "http://127.0.0.1:9",
                dir.path(),
                SessionIdMode::Default,
            )
            .expect("state");
            state
                .create_session()
                .await
                .expect("session survives bad seed");
            let diagnostics = fs::read_to_string(dir.path().join("diagnostics/default.jsonl"))
                .expect("diagnostics");
            assert!(diagnostics.contains("volition_continuity_note"));
            assert!(diagnostics.contains(if malformed {
                "could not be loaded"
            } else {
                "discarded fixture-incompatible"
            }));
        }
    }

    #[test]
    fn continuity_manifest_template_rejects_unknown_schema_and_resume_mode() {
        let unknown_schema = br#"{"schema_version":999,"current_session_id":null,"current_session_state_path":null,"current_volition_snapshot_path":null,"last_sleep_run_id":null,"last_sleep_brief_path":null,"last_sleep_consumed_session_id":null,"sleep_pending":false,"resume_mode":"cold_start"}"#;
        assert!(
            parse_manifest_template(unknown_schema)
                .expect_err("schema")
                .to_string()
                .contains("unsupported seed continuity manifest schema_version")
        );
        let unknown_mode = String::from_utf8(unknown_schema.to_vec())
            .expect("utf8")
            .replace("999", "1")
            .replace("cold_start", "invented");
        assert!(parse_manifest_template(unknown_mode.as_bytes()).is_err());
    }
}
