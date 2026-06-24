use super::{DocKind, MaturityTag};

pub fn kind_for_path(path: &str) -> DocKind {
    if path.starts_with("docs/ProjectFrame/") || path == "README.md" {
        return DocKind::Frame;
    }
    if path.starts_with("docs/Concepts/") {
        return DocKind::Concept;
    }
    if path.starts_with("docs/Research/") {
        return DocKind::Research;
    }
    if let Some(rest) = path.strip_prefix("docs/Plans/") {
        if rest.starts_with("Plan.") {
            return DocKind::Plan;
        }
        if rest.starts_with("Idea.") {
            return DocKind::Idea;
        }
        if rest.starts_with("Design.") {
            return DocKind::Design;
        }
    }
    if path.starts_with("docs/Architecture/") {
        return DocKind::Architecture;
    }
    if let Some(rest) = path.strip_prefix("docs/Experiments/") {
        if rest.starts_with("Experiment.") {
            return DocKind::ExperimentSpec;
        }
        if rest.starts_with("Report.") {
            return DocKind::ExperimentReport;
        }
    }
    if path == "docs/DecisionLog.md" {
        return DocKind::Decision;
    }
    DocKind::Unknown
}

pub fn maturity_for(kind: DocKind, body: &str) -> MaturityTag {
    match kind {
        DocKind::Concept | DocKind::Architecture => maturity_from_section(body, "Maturity"),
        DocKind::Design => maturity_from_section(body, "Status"),
        _ => MaturityTag::NotApplicable,
    }
}

pub fn last_reviewed_for(body: &str) -> Option<String> {
    let section = section_text_by_heading(body, "Implementation Status")?;
    for line in section.lines().skip(1) {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("Last reviewed:") else {
            continue;
        };
        let token = rest
            .split_whitespace()
            .next()?
            .trim_end_matches(['.', ',', ';']);
        if is_iso_date(token) {
            return Some(token.to_string());
        }
    }
    None
}

fn maturity_from_section(body: &str, heading: &str) -> MaturityTag {
    let Some(section) = section_text_by_heading(body, heading) else {
        return MaturityTag::Unknown;
    };

    for line in section.lines().skip(1) {
        let value = line.trim();
        if value.is_empty() {
            continue;
        }
        let token = value
            .split_whitespace()
            .next()
            .unwrap_or(value)
            .trim_end_matches(['.', ',', ';']);
        return match token {
            "Brainstorm" => MaturityTag::Brainstorm,
            "Sketch" => MaturityTag::Sketch,
            "Candidate" => MaturityTag::Candidate,
            "Accepted" => MaturityTag::Accepted,
            "Implemented" => MaturityTag::Implemented,
            "Deprecated" => MaturityTag::Deprecated,
            _ => MaturityTag::Unknown,
        };
    }

    MaturityTag::Unknown
}

fn section_text_by_heading<'a>(body: &'a str, heading: &str) -> Option<&'a str> {
    let mut current_start: Option<usize> = None;
    let mut current_heading: Option<&str> = None;
    let mut offset = 0usize;

    for segment in body.split_inclusive('\n') {
        let trimmed = segment.trim_start();
        if trimmed.starts_with("## ") {
            if let (Some(start), Some(found_heading)) = (current_start, current_heading) {
                if found_heading == heading {
                    return Some(&body[start..offset]);
                }
            }

            current_start = Some(offset);
            current_heading = Some(trimmed.strip_prefix("## ").unwrap().trim());
        }
        offset += segment.len();
    }

    if let (Some(start), Some(found_heading)) = (current_start, current_heading) {
        if found_heading == heading {
            return Some(&body[start..offset]);
        }
    }

    None
}

fn is_iso_date(token: &str) -> bool {
    if token.len() != 10 {
        return false;
    }

    let bytes = token.as_bytes();
    bytes[0..4].iter().all(|byte| byte.is_ascii_digit())
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(|byte| byte.is_ascii_digit())
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{DocKind, MaturityTag};

    #[test]
    fn kind_from_projectframe_path() {
        assert_eq!(
            kind_for_path("docs/ProjectFrame/ProjectVision.md"),
            DocKind::Frame
        );
    }

    #[test]
    fn kind_from_plan_idea_design_filenames() {
        assert_eq!(kind_for_path("docs/Plans/Plan.X.md"), DocKind::Plan);
        assert_eq!(kind_for_path("docs/Plans/Idea.X.md"), DocKind::Idea);
        assert_eq!(kind_for_path("docs/Plans/Design.X.md"), DocKind::Design);
    }

    #[test]
    fn kind_falls_back_to_unknown() {
        assert_eq!(kind_for_path("docs/Random/Other.md"), DocKind::Unknown);
    }

    #[test]
    fn maturity_from_concept_doc() {
        let body = include_str!("fixtures/sample_concept.md");
        assert_eq!(maturity_for(DocKind::Concept, body), MaturityTag::Candidate);
    }

    #[test]
    fn maturity_from_design_uses_status_heading() {
        let body = include_str!("fixtures/sample_design.md");
        assert_eq!(maturity_for(DocKind::Design, body), MaturityTag::Candidate);
    }

    #[test]
    fn maturity_unknown_when_heading_missing() {
        let body = include_str!("fixtures/sample_unknown.md");
        assert_eq!(maturity_for(DocKind::Concept, body), MaturityTag::Unknown);
    }

    #[test]
    fn maturity_unknown_for_unrecognized_value() {
        let body = "## Maturity\n\nBananas\n";
        assert_eq!(maturity_for(DocKind::Concept, body), MaturityTag::Unknown);
    }

    #[test]
    fn maturity_uses_first_token_with_trailing_punctuation() {
        let body = "## Maturity\n\nCandidate because this is still being evaluated.\n";
        assert_eq!(maturity_for(DocKind::Concept, body), MaturityTag::Candidate);

        let punctuated = "## Maturity\n\nAccepted.\n";
        assert_eq!(
            maturity_for(DocKind::Architecture, punctuated),
            MaturityTag::Accepted
        );
    }

    #[test]
    fn maturity_not_applicable_for_frame() {
        assert_eq!(
            maturity_for(DocKind::Frame, "anything"),
            MaturityTag::NotApplicable
        );
    }

    #[test]
    fn last_reviewed_parsed_from_architecture() {
        let body = include_str!("fixtures/sample_architecture.md");
        assert_eq!(last_reviewed_for(body).as_deref(), Some("2026-05-21"));
    }

    #[test]
    fn last_reviewed_none_when_section_absent() {
        let body = include_str!("fixtures/sample_concept.md");
        assert_eq!(last_reviewed_for(body), None);
    }

    #[test]
    fn last_reviewed_ignored_outside_implementation_status_section() {
        let body = "# Doc\n\n## Body\n\nLast reviewed: 2020-01-01 somewhere.\n";
        assert_eq!(last_reviewed_for(body), None);
    }

    #[test]
    fn last_reviewed_none_for_malformed_date() {
        let body = "## Implementation Status\n\nLast reviewed: May 2026.\n";
        assert_eq!(last_reviewed_for(body), None);
    }

    #[test]
    fn last_reviewed_tolerates_trailing_punctuation() {
        let body = "## Implementation Status\n\nLast reviewed: 2026-05-21.\n";
        assert_eq!(last_reviewed_for(body).as_deref(), Some("2026-05-21"));
    }
}
