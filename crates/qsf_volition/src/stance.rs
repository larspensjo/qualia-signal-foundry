use sha2::Digest;

use crate::{Mode, VolitionFixture};

const RENDER_WIDTH: usize = 80;

/// Render the configured volition stance as bounded text with no session-specific facts.
pub fn render_volition_stance(fixture: &VolitionFixture, mode: Mode) -> String {
    let mut lines = Vec::new();
    lines.extend(wrap_paragraph(
        "",
        "Simulated volition stance (internal state only — not a claim of real desire, consciousness, or subjective experience).",
        RENDER_WIDTH,
        "",
    ));
    lines.push("Configured tensions, most protected first:".to_string());

    let mut tensions = fixture.tensions.clone();
    tensions.sort_by(|left, right| {
        left.arbitration_tier
            .cmp(&right.arbitration_tier)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });

    for tension in tensions {
        let prefix = format!("- [tier {}] {}: ", tension.arbitration_tier, tension.title);
        lines.extend(wrap_paragraph(
            &prefix,
            tension.summary.trim(),
            RENDER_WIDTH,
            "  ",
        ));
    }

    lines.extend(wrap_paragraph(
        "Arbitration stance: ",
        &format!(
            "tiers at or below 3 are protected and outrank curiosity and exploration under every mode. Default mode: {}.",
            mode
        ),
        RENDER_WIDTH,
        "",
    ));

    lines.join("\n")
}

pub fn stable_baseline_hash(instructions: &str) -> String {
    let hash = sha2::Sha256::digest(instructions.as_bytes());
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn wrap_paragraph(
    prefix: &str,
    body: &str,
    width: usize,
    continuation_prefix: &str,
) -> Vec<String> {
    if body.trim().is_empty() {
        return vec![prefix.trim_end().to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut available = width.saturating_sub(prefix.len());
    let mut line_prefix = prefix.to_string();

    for word in body.split_whitespace() {
        let projected = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if projected > available && !current.is_empty() {
            lines.push(format!("{line_prefix}{current}"));
            line_prefix = continuation_prefix.to_string();
            available = width.saturating_sub(continuation_prefix.len());
            current.clear();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    lines.push(format!("{line_prefix}{current}"));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mode, realtime_seed_fixture};

    #[test]
    fn stance_rendering_is_deterministic() {
        let fixture = realtime_seed_fixture();
        let first = render_volition_stance(&fixture, Mode::Neutral);
        let second = render_volition_stance(&fixture, Mode::Neutral);
        assert_eq!(first, second);
    }

    #[test]
    fn stance_does_not_claim_real_desire() {
        let fixture = realtime_seed_fixture();
        let rendered = render_volition_stance(&fixture, Mode::Neutral);
        assert!(rendered.contains("not a claim of"));
        assert!(rendered.contains("consciousness, or subjective experience"));
        assert!(rendered.contains("not a claim of real desire"));
    }

    #[test]
    fn stance_renders_most_protected_tension_first() {
        let fixture = realtime_seed_fixture();
        let rendered = render_volition_stance(&fixture, Mode::Neutral);
        let min_tier = fixture
            .tensions
            .iter()
            .map(|t| t.arbitration_tier)
            .min()
            .unwrap();
        let first_tension_line = rendered
            .lines()
            .find(|l| l.trim_start().starts_with("- [tier "))
            .expect("stance must render at least one tension line");
        assert!(
            first_tension_line.contains(&format!("[tier {min_tier}]")),
            "first rendered tension must carry the minimum tier; got: {first_tension_line}"
        );
        assert!(rendered.contains("Default mode: Neutral."));
    }

    #[test]
    fn baseline_hash_is_deterministic() {
        let fixture = realtime_seed_fixture();
        let rendered = render_volition_stance(&fixture, Mode::Neutral);
        assert_eq!(
            stable_baseline_hash(&rendered),
            stable_baseline_hash(&rendered)
        );
    }
}
