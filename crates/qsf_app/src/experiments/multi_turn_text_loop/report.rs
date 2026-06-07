use std::fs;

use crate::conversation::prompt;
use crate::runtime::run_context::RunContext;
use crate::session::SessionState;
use anyhow::Context;

use super::{SessionMemorySourceSnapshot, assemble_session_prompt};

pub(crate) fn write_multi_turn_report(
    context: &RunContext,
    state: &SessionState,
    memory_snapshot: &SessionMemorySourceSnapshot,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Multi-Turn Text Loop\n\n");
    markdown.push_str("## Configuration\n\n");
    markdown.push_str(&format!("- Model: `{}`\n", state.config.model_id));
    markdown.push_str(&format!("- Max turns: `{}`\n", state.config.max_turns));
    markdown.push_str(&format!(
        "- Warm threshold: `{}` active verbatim turns\n",
        state.config.warm_threshold
    ));
    markdown.push_str(&format!(
        "- Allow over limit: `{}`\n",
        state.config.allow_over_limit
    ));
    markdown.push_str(&format!(
        "- Requested memory source: `{}`\n",
        state.config.memory_source.source
    ));
    markdown.push_str(&format!(
        "- Loaded memory source: `{}`\n",
        memory_snapshot.source_name
    ));
    markdown.push_str(&format!(
        "- Loaded memory source reference: `{}`\n\n",
        memory_snapshot.source_reference
    ));
    markdown.push_str("## Turns\n\n");
    markdown.push_str("| Turn | Input tokens | Cached input tokens | Cache ratio | Output tokens | Latency ms | Hash prefix status |\n");
    markdown.push_str("|---:|---:|---:|---:|---:|---:|---|\n");

    for (index, turn) in state.turns.iter().enumerate() {
        let ratio = if turn.input_tokens == 0 {
            0.0
        } else {
            f64::from(turn.cached_input_tokens) / f64::from(turn.input_tokens)
        };
        let prefix_status = prompt_prefix_status_for_report(state, index);
        markdown.push_str(&format!(
            "| {} | {} | {} | {:.2} | {} | {} | {} |\n",
            turn.index,
            turn.input_tokens,
            turn.cached_input_tokens,
            ratio,
            turn.output_tokens,
            turn.model_latency_ms,
            prefix_status
        ));
    }

    markdown.push_str("\n## Cache Diagnostics\n\n");
    let cache_misses_above_floor = state
        .turns
        .iter()
        .filter(|turn| turn.input_tokens >= 1024 && turn.cached_input_tokens == 0)
        .count();
    markdown.push_str(&format!(
        "- Cache misses at or above 1024 input tokens: `{cache_misses_above_floor}`\n"
    ));
    markdown.push_str("- Prompt cache floor: `1024` input tokens\n");
    markdown.push_str(&format!(
        "- Warm summaries produced: `{}`\n",
        state.summarized_turns.len()
    ));
    let recall_count = state
        .turns
        .iter()
        .map(|turn| turn.recalled_turns.len())
        .sum::<usize>();
    markdown.push_str(&format!("- Recall tool executions: `{recall_count}`\n"));
    markdown.push_str("- Session state persistence: `continuity_manifest`\n\n");
    markdown.push_str("## Warm Summaries\n\n");
    if state.summarized_turns.is_empty() {
        markdown.push_str("- None\n\n");
    } else {
        markdown.push_str(
            "| Turn | Summary model | Latency ms | Input tokens | Output tokens | Summary |\n",
        );
        markdown.push_str("|---:|---|---:|---:|---:|---|\n");
        for summary in &state.summarized_turns {
            markdown.push_str(&format!(
                "| {} | `{}` | {} | {} | {} | {} |\n",
                summary.turn_index,
                summary.model_id,
                summary.model_latency_ms,
                summary.input_tokens,
                summary.output_tokens,
                summary.summary.replace('|', "\\|")
            ));
        }
        markdown.push('\n');
    }
    markdown.push_str("## Recall Tool\n\n");
    if recall_count == 0 {
        markdown.push_str("- None\n\n");
    } else {
        markdown.push_str("| Turn | Recalled turn | Call id | Latency ms |\n");
        markdown.push_str("|---:|---:|---|---:|\n");
        for turn in &state.turns {
            for recall in &turn.recalled_turns {
                markdown.push_str(&format!(
                    "| {} | {} | `{}` | {} |\n",
                    turn.index, recall.turn_id, recall.call_id, recall.latency_ms
                ));
            }
        }
        markdown.push('\n');
    }
    markdown.push_str("## Hashes\n\n");
    for turn in &state.turns {
        markdown.push_str(&format!(
            "- Turn {}: `{}` messages=`{}`\n",
            turn.index, turn.full_request_hash, turn.message_count
        ));
    }

    fs::write(context.run_dir().join("multi-turn-text-loop.md"), markdown)
        .context("failed to write multi-turn text loop report")
}

pub(crate) fn prompt_prefix_status_for_report(
    state: &SessionState,
    turn_position: usize,
) -> String {
    if turn_position == 0 {
        return "n/a".to_string();
    }

    let previous = &state.turns[turn_position - 1];
    if state
        .summarized_turns
        .iter()
        .any(|summary| summary.summarized_after_turn_index == previous.index)
    {
        return "invalidated_by_warm_summary".to_string();
    }
    if let Some(invalidation) = state
        .prompt_prefix_invalidations
        .iter()
        .find(|invalidation| invalidation.after_turn_index == previous.index)
    {
        return format!("invalidated_by_{}", invalidation.reason);
    }

    let prompt_state = SessionState {
        turns: state.turns[..turn_position].to_vec(),
        summarized_turns: state
            .summarized_turns
            .iter()
            .filter(|summary| summary.summarized_after_turn_index < turn_position)
            .cloned()
            .collect(),
        live: crate::session::LiveSessionState::default(),
        ..state.clone()
    };
    let turn = &state.turns[turn_position];
    let prompt_assembly = assemble_session_prompt(
        &prompt_state,
        &turn.user_input,
        &turn.retrieved_memory_block,
        true,
    );

    (prompt::prior_request_prefix_hash(&prompt_assembly.messages, previous.message_count)
        == Some(previous.full_request_hash))
    .to_string()
}
