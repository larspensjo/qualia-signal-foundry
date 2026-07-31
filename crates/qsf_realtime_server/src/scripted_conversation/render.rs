use super::{FormationState, PhraseSet, ProbeVerdict};

pub struct ProbeHeader<'a> {
    pub run_id: &'a str,
    pub state_dir: &'a str,
    pub phrases: &'a PhraseSet,
    pub hash: &'a str,
    pub model: &'a str,
    pub attachment_shape: &'a str,
    pub reconnect_policy: &'a str,
    pub world_corpus: &'a str,
    pub seed_mode: &'a str,
}

pub fn render_header(header: &ProbeHeader<'_>) -> String {
    format!(
        "probe {}: {} phrases from {} ({}), state {}, model {}, attachment {}, reconnect {}, world corpus {}, seed {}",
        header.run_id,
        header.phrases.phrases.len(),
        header.phrases.id,
        header.hash,
        header.state_dir,
        header.model,
        header.attachment_shape,
        header.reconnect_policy,
        header.world_corpus,
        header.seed_mode,
    )
}

pub fn render_turn(
    index: usize,
    total: usize,
    text: &str,
    elapsed_ms: u64,
    promoted: bool,
    arbitration_winner: Option<&str>,
) -> String {
    let preview: String = text.chars().take(72).collect();
    format!(
        "turn {}/{}: {} ({} ms, promoted: {}, winner: {})",
        index + 1,
        total,
        preview,
        elapsed_ms,
        promoted,
        arbitration_winner.unwrap_or("none")
    )
}

pub fn render_formation_barrier(formation: Option<&FormationState>) -> String {
    match formation {
        Some(formation) if formation.timed_out => format!(
            "WARNING: formation barrier timed out after {} ms ({}/{} settled, {} failed)",
            formation.timeout_ms, formation.settled, formation.expected, formation.failed
        ),
        Some(formation) => format!(
            "formation barrier: {}/{} settled, {} failed (timeout {} ms)",
            formation.settled, formation.expected, formation.failed, formation.timeout_ms
        ),
        None => "formation barrier: not observed".to_string(),
    }
}

pub fn render_structured_partial_warning(clause: &str) -> String {
    format!("WARNING: structured partial: {clause}")
}

pub fn render_verdict(verdict: &ProbeVerdict) -> String {
    format!(
        "probe {:?}; failures: {}; structured partials: {}",
        verdict.status,
        verdict.failing_clauses.join(", "),
        verdict.structured_clauses.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_and_structured_partial_are_prominent() {
        let formation = FormationState {
            expected: 2,
            settled: 1,
            failed: 1,
            timed_out: true,
            timeout_ms: 50,
        };

        assert!(render_formation_barrier(Some(&formation)).starts_with("WARNING:"));
        assert!(render_structured_partial_warning("formation_timed_out").starts_with("WARNING:"));
    }
}
