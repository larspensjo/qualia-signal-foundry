use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::association::Association;
use super::memory_record::{MemoryRecord, MemoryRecordKind};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemoryFixture {
    pub records: Vec<MemoryRecord>,
    pub associations: Vec<Association>,
}

pub fn phase_four_fixture() -> MemoryFixture {
    let records = vec![
        MemoryRecord::new(
            "memory.associative-memory",
            MemoryRecordKind::Concept,
            "Associative memory",
            "Explicit weighted links should make indirect recall inspectable and comparable against recency and keyword baselines.",
            vec!["memory", "association", "retrieval", "trace"],
            timestamp("2026-05-01T09:00:00Z"),
            0.95,
            3,
            "docs/Concepts/Concept.AssociativeMemory.md",
            42,
        ),
        MemoryRecord::new(
            "memory.context-budget",
            MemoryRecordKind::Concept,
            "Context budget",
            "The live loop should select compact context under explicit token and fragment budgets instead of loading everything.",
            vec!["context", "budget", "retrieval", "selection"],
            timestamp("2026-05-02T09:00:00Z"),
            0.9,
            2,
            "docs/Concepts/Concept.ContextBudget.md",
            38,
        ),
        MemoryRecord::new(
            "memory.sleep-phase",
            MemoryRecordKind::Concept,
            "Sleep phase consolidation",
            "Sleep-phase work can summarize sessions, propose memory candidates, and create associations without silently accepting decisions.",
            vec!["sleep", "summary", "memory", "consolidation"],
            timestamp("2026-05-03T09:00:00Z"),
            0.78,
            1,
            "docs/Concepts/Concept.SleepPhase.md",
            44,
        ),
        MemoryRecord::new(
            "memory.tools-as-perception",
            MemoryRecordKind::Concept,
            "Tools as perception",
            "Early tools should be read-only or compute-only observations that feed structured results back into the runtime.",
            vec!["tools", "perception", "safety", "context"],
            timestamp("2026-05-04T09:00:00Z"),
            0.74,
            1,
            "docs/Concepts/Concept.ToolsAsPerception.md",
            36,
        ),
        MemoryRecord::new(
            "memory.observability",
            MemoryRecordKind::ArchitectureNote,
            "Structured observability",
            "Developer logs, event logs, and trace logs are separate artifacts with different responsibilities.",
            vec!["observability", "event-log", "trace", "report"],
            timestamp("2026-05-05T09:00:00Z"),
            0.86,
            2,
            "docs/Architecture/Architecture.StateAndObservability.md",
            35,
        ),
        MemoryRecord::new(
            "memory.runtime-flow",
            MemoryRecordKind::Decision,
            "Unidirectional runtime flow",
            "Runtime state changes through pure reducers; side effects return as events before state changes.",
            vec!["runtime", "reducer", "events", "state"],
            timestamp("2026-05-06T09:00:00Z"),
            0.98,
            4,
            "docs/DecisionLog.md",
            32,
        ),
        MemoryRecord::new(
            "memory.model-roles",
            MemoryRecordKind::ArchitectureNote,
            "Model roles",
            "Different model calls should describe why they are being made and what output they expect.",
            vec!["model", "roles", "mock", "openai"],
            timestamp("2026-05-07T09:00:00Z"),
            0.65,
            0,
            "docs/Architecture/Architecture.ModelRoles.md",
            30,
        ),
        MemoryRecord::new(
            "memory.audio-loop",
            MemoryRecordKind::Experiment,
            "Transcript-first audio loop",
            "Realtime speech integration starts with transcript events before full speech-to-speech presence.",
            vec!["audio", "transcript", "realtime", "events"],
            timestamp("2026-05-08T09:00:00Z"),
            0.72,
            1,
            "docs/Experiments/Experiment.StreamingTranscriptionMVP.md",
            34,
        ),
        MemoryRecord::new(
            "memory.external-inputs",
            MemoryRecordKind::Question,
            "External inputs",
            "External signals need to become structured input events instead of bypassing the runtime loop.",
            vec!["external-inputs", "events", "runtime", "perception"],
            timestamp("2026-05-09T09:00:00Z"),
            0.68,
            0,
            "docs/Concepts/Concept.ExternalInputs.md",
            31,
        ),
        MemoryRecord::new(
            "memory.non-goals",
            MemoryRecordKind::Observation,
            "Framework MVP non-goals",
            "The MVP defers production storage, full agent autonomy, video input, cloud deployment, and polished UI.",
            vec!["non-goals", "scope", "mvp", "safety"],
            timestamp("2026-05-10T09:00:00Z"),
            0.8,
            1,
            "docs/ProjectFrame/NonGoals.md",
            33,
        ),
    ];

    let associations = vec![
        Association::new(
            "memory.associative-memory",
            "memory.context-budget",
            0.92,
            "retrieved memories must compete for scarce context",
            timestamp("2026-05-08T10:00:00Z"),
        ),
        Association::new(
            "memory.context-budget",
            "memory.associative-memory",
            0.88,
            "context selection needs memory candidates",
            timestamp("2026-05-08T10:00:00Z"),
        ),
        Association::new(
            "memory.associative-memory",
            "memory.sleep-phase",
            0.72,
            "sleep may create or reinforce associations",
            timestamp("2026-05-08T10:00:00Z"),
        ),
        Association::new(
            "memory.runtime-flow",
            "memory.external-inputs",
            0.86,
            "external providers must feed events into the reducer path",
            timestamp("2026-05-09T10:00:00Z"),
        ),
        Association::new(
            "memory.external-inputs",
            "memory.audio-loop",
            0.78,
            "transcripts are external input events",
            timestamp("2026-05-09T10:00:00Z"),
        ),
        Association::new(
            "memory.tools-as-perception",
            "memory.external-inputs",
            0.67,
            "tools and sensors are both perception-like inputs",
            timestamp("2026-05-09T10:00:00Z"),
        ),
        Association::new(
            "memory.observability",
            "memory.associative-memory",
            0.7,
            "retrieval quality depends on inspectable traces",
            timestamp("2026-05-09T10:00:00Z"),
        ),
        Association::new(
            "memory.context-budget",
            "memory.observability",
            0.64,
            "selected and omitted context must be visible",
            timestamp("2026-05-09T10:00:00Z"),
        ),
        Association::new(
            "memory.non-goals",
            "memory.tools-as-perception",
            0.56,
            "safe compute-only tools match MVP scope boundaries",
            timestamp("2026-05-10T10:00:00Z"),
        ),
    ];

    MemoryFixture {
        records,
        associations,
    }
}

fn timestamp(value: &str) -> OffsetDateTime {
    OffsetDateTime::parse(value, &Rfc3339).expect("fixture timestamp must be valid RFC3339")
}

#[cfg(test)]
mod tests {
    use super::phase_four_fixture;
    use crate::memory::association::ensure_current_association_schema;
    use crate::memory::memory_record::ensure_current_memory_schema;

    #[test]
    fn phase_four_fixture_uses_current_schemas() {
        let fixture = phase_four_fixture();

        assert!(ensure_current_memory_schema(&fixture.records).is_ok());
        assert!(ensure_current_association_schema(&fixture.associations).is_ok());
    }
}
