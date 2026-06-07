use std::io::Write;

use crate::console::styling::ColorMode;
use crate::context::{ContextAssembly, ContextSourceKind};

pub(crate) fn begin_user_input_echo<W: Write>(
    output: &mut W,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{STYLE_USER_INPUT, style_prefix};

    write!(output, "{}", style_prefix(color_mode, STYLE_USER_INPUT))?;
    output.flush()
}

pub(crate) fn end_user_input_echo<W: Write>(
    output: &mut W,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::style_reset;

    write!(output, "{}", style_reset(color_mode))?;
    output.flush()
}

pub(crate) fn print_assistant_response<W: Write>(
    output: &mut W,
    response: &str,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{STYLE_ASSISTANT_RESPONSE, paint};

    writeln!(
        output,
        "{}",
        paint(color_mode, STYLE_ASSISTANT_RESPONSE, response)
    )
}

pub(crate) fn print_memory_blocks<W: Write>(
    output: &mut W,
    assembly: &ContextAssembly,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{
        STYLE_DIRECT_BODY, STYLE_DIRECT_HEADER, STYLE_HINT_BLOCK, paint,
    };

    let mut directs: Vec<String> = Vec::new();
    let mut hints: Vec<String> = Vec::new();

    for selection in &assembly.selected {
        let line = format!(
            "- {}: {}",
            selection.fragment.fragment_id, selection.fragment.summary
        );
        match selection.fragment.source_kind {
            ContextSourceKind::Memory => directs.push(line),
            ContextSourceKind::MemoryHint => hints.push(line),
            _ => {}
        }
    }

    if !directs.is_empty() {
        writeln!(
            output,
            "{}",
            paint(
                color_mode,
                STYLE_DIRECT_HEADER,
                "=== Memories retrieved for this turn ===",
            )
        )?;
        for line in &directs {
            writeln!(output, "{}", paint(color_mode, STYLE_DIRECT_BODY, line))?;
        }
    }

    if !hints.is_empty() {
        writeln!(
            output,
            "{}",
            paint(
                color_mode,
                STYLE_HINT_BLOCK,
                "=== Associated memories (hints - may or may not be relevant) ===",
            )
        )?;
        for line in &hints {
            writeln!(output, "{}", paint(color_mode, STYLE_HINT_BLOCK, line))?;
        }
    }

    Ok(())
}
