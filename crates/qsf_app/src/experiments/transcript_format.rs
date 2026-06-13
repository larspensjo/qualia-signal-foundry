pub(super) fn append_labelled_value(
    transcript: &mut String,
    label: &str,
    value: &str,
    placeholder: &str,
) {
    transcript.push_str(label);
    transcript.push_str(":\n");
    let rendered = if value.trim().is_empty() {
        placeholder
    } else {
        value.trim()
    };
    transcript.push_str(rendered);
    transcript.push('\n');
}

pub(super) fn append_retrieved_memory_block(transcript: &mut String, retrieved_memory_block: &str) {
    if retrieved_memory_block.trim().is_empty() {
        return;
    }

    transcript.push_str("Retrieved memory block:\n");
    transcript.push_str(retrieved_memory_block.trim());
    transcript.push('\n');
}

pub(super) fn append_recalled_items(
    transcript: &mut String,
    label: &str,
    recalled_items: &[crate::session::RecallRecord],
) {
    if recalled_items.is_empty() {
        return;
    }

    transcript.push_str(label);
    transcript.push_str(":\n");
    for recall in recalled_items {
        transcript.push_str(&format!(
            "- {} recalled turn {} via {}\n",
            recall.call_id, recall.turn_id, recall.tool_name
        ));
    }
}
