use qsf_session::context::{ContextAssembly, ContextOmission, ContextSelection};

use super::context_budget::ContextBudget;
use super::context_fragment::ContextFragment;

pub fn assemble_context(fragments: Vec<ContextFragment>, budget: ContextBudget) -> ContextAssembly {
    let mut sorted = fragments;
    sorted.sort_by(|left, right| {
        right
            .source_kind
            .source_priority()
            .cmp(&left.source_kind.source_priority())
            .then_with(|| right.score.total_cmp(&left.score))
            .then_with(|| left.estimated_tokens.cmp(&right.estimated_tokens))
            .then_with(|| left.fragment_id.cmp(&right.fragment_id))
    });

    let mut selected = Vec::new();
    let mut omitted = Vec::new();
    let mut used_estimated_tokens = 0;

    for fragment in sorted {
        if selected.len() >= budget.max_fragments {
            omitted.push(ContextOmission {
                fragment,
                reason: "fragment limit reached".to_string(),
            });
            continue;
        }

        let next_token_total = used_estimated_tokens + fragment.estimated_tokens;
        if next_token_total > budget.max_estimated_tokens {
            omitted.push(ContextOmission {
                fragment,
                reason: format!(
                    "token budget exceeded: would use {} of {}",
                    next_token_total, budget.max_estimated_tokens
                ),
            });
            continue;
        }

        used_estimated_tokens = next_token_total;
        selected.push(ContextSelection {
            fragment,
            cumulative_estimated_tokens: used_estimated_tokens,
        });
    }

    ContextAssembly {
        budget,
        selected,
        omitted,
        used_estimated_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::assemble_context;
    use crate::context::{ContextBudget, ContextFragment, ContextSourceKind};

    #[test]
    fn assembler_respects_fragment_and_token_budget() {
        let fragments = vec![
            fragment("a", 10.0, 40),
            fragment("b", 9.0, 35),
            fragment("c", 8.0, 50),
        ];
        let assembly = assemble_context(fragments, ContextBudget::new(2, 75));

        assert_eq!(assembly.selected.len(), 2);
        assert_eq!(assembly.omitted.len(), 1);
        assert_eq!(assembly.used_estimated_tokens, 75);
        assert_eq!(assembly.omitted[0].reason, "fragment limit reached");
    }

    #[test]
    fn assembler_logs_token_budget_omissions() {
        let fragments = vec![fragment("a", 10.0, 70), fragment("b", 9.0, 40)];
        let assembly = assemble_context(fragments, ContextBudget::new(3, 80));

        assert_eq!(assembly.selected.len(), 1);
        assert_eq!(assembly.omitted.len(), 1);
        assert!(assembly.omitted[0].reason.contains("token budget exceeded"));
    }

    #[test]
    fn hint_cannot_evict_direct_under_budget_pressure() {
        let direct = fragment_with_kind("direct.a", 5.0, 60, ContextSourceKind::Memory);
        let hint = fragment_with_kind("hint.b", 10.0, 60, ContextSourceKind::MemoryHint);

        let assembly = assemble_context(vec![hint, direct], ContextBudget::new(2, 60));

        assert_eq!(assembly.selected.len(), 1);
        assert_eq!(assembly.selected[0].fragment.fragment_id, "direct.a");
        assert_eq!(assembly.omitted.len(), 1);
        assert_eq!(assembly.omitted[0].fragment.fragment_id, "hint.b");
    }

    fn fragment(id: &str, score: f64, estimated_tokens: usize) -> ContextFragment {
        fragment_with_kind(id, score, estimated_tokens, ContextSourceKind::Memory)
    }

    fn fragment_with_kind(
        id: &str,
        score: f64,
        estimated_tokens: usize,
        source_kind: ContextSourceKind,
    ) -> ContextFragment {
        ContextFragment {
            fragment_id: id.to_string(),
            source_kind,
            summary: format!("Fragment {id}"),
            tags: vec![],
            score,
            estimated_tokens,
            source_reference: "test".to_string(),
            selection_reason: "test".to_string(),
        }
    }
}
