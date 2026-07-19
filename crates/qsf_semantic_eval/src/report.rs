use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{GoldLabel, PairResult};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BinaryMetrics {
    pub threshold: u32,
    pub true_positive: u32,
    pub false_positive: u32,
    pub false_negative: u32,
    pub true_negative: u32,
    pub precision: Option<Ratio>,
    pub recall: Option<Ratio>,
    pub f1: Option<Ratio>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Ratio(pub f64);

impl Eq for Ratio {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ThresholdMetrics {
    pub threshold: u32,
    pub metrics: BinaryMetrics,
    pub production_operating_point: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParaphraseClusterRecall {
    pub cluster_id: String,
    pub relevant_pair_count: u32,
    pub recalled_relevant_pair_count: u32,
    pub recall: Option<Ratio>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SliceBreakdown {
    pub slice_name: String,
    pub total_pair_count: u32,
    pub ambiguous_pair_count: u32,
    pub binary_metrics_at_operating_point: BinaryMetrics,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeferredMetric {
    pub status: String,
    pub measurement_method: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MetricsReport {
    pub dataset_version: String,
    pub roster_snapshot_hash: String,
    pub production_operating_point_threshold: u32,
    pub threshold_sweep: Vec<ThresholdMetrics>,
    pub production_operating_point: BinaryMetrics,
    pub paraphrase_cluster_recall: Vec<ParaphraseClusterRecall>,
    pub slice_breakdowns: Vec<SliceBreakdown>,
    pub ambiguous_pair_count: u32,
    pub none_of_roster_pair_count: u32,
    pub latency: DeferredMetric,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedReportArtifacts {
    pub pair_results_jsonl: PathBuf,
    pub metrics_json: PathBuf,
    pub metrics_summary: PathBuf,
    pub error_analysis: PathBuf,
}

pub fn build_metrics_report(
    results: &[PairResult],
    none_of_roster_pair_count: u32,
) -> Result<MetricsReport, String> {
    let first = results
        .first()
        .ok_or_else(|| "cannot report on zero pair results".to_string())?;
    if results
        .iter()
        .any(|result| result.dataset_version != first.dataset_version)
    {
        return Err("pair results contain more than one dataset_version".to_string());
    }
    if results
        .iter()
        .any(|result| result.roster_snapshot_hash != first.roster_snapshot_hash)
    {
        return Err("pair results contain more than one roster_snapshot_hash".to_string());
    }
    let threshold = first.qualification_threshold_in_force;
    if results
        .iter()
        .any(|result| result.qualification_threshold_in_force != threshold)
    {
        return Err("pair results contain more than one qualification threshold".to_string());
    }
    let maximum_strength = results
        .iter()
        .map(|result| result.match_strength)
        .max()
        .unwrap_or(0);
    let threshold_sweep = (0..=maximum_strength.max(threshold))
        .map(|value| ThresholdMetrics {
            threshold: value,
            metrics: binary_metrics(results, value),
            production_operating_point: value == threshold,
        })
        .collect();
    let slice_names = required_and_observed_slices(results);
    let slice_breakdowns = slice_names
        .into_iter()
        .map(|slice_name| {
            let members = results
                .iter()
                .filter(|result| belongs_to_report_slice(result, &slice_name))
                .cloned()
                .collect::<Vec<_>>();
            SliceBreakdown {
                slice_name,
                total_pair_count: members.len() as u32,
                ambiguous_pair_count: members
                    .iter()
                    .filter(|result| result.gold_label == GoldLabel::Ambiguous)
                    .count() as u32,
                binary_metrics_at_operating_point: binary_metrics(&members, threshold),
            }
        })
        .collect();
    Ok(MetricsReport {
        dataset_version: first.dataset_version.clone(),
        roster_snapshot_hash: first.roster_snapshot_hash.clone(),
        production_operating_point_threshold: threshold,
        production_operating_point: binary_metrics(results, threshold),
        paraphrase_cluster_recall: paraphrase_cluster_recall(results, threshold),
        slice_breakdowns,
        ambiguous_pair_count: results
            .iter()
            .filter(|result| result.gold_label == GoldLabel::Ambiguous)
            .count() as u32,
        none_of_roster_pair_count,
        threshold_sweep,
        latency: DeferredMetric {
            status: "deferred".to_string(),
            measurement_method: "Later measurement times only qsf_volition scoring calls per utterance after warm-up, excluding dataset I/O, and records p50/p95/p99 with machine and build profile.".to_string(),
        },
    })
}

pub fn write_report_artifacts(
    output_dir: impl AsRef<Path>,
    results: &[PairResult],
    report: &MetricsReport,
) -> Result<GeneratedReportArtifacts, String> {
    let output_dir = output_dir.as_ref();
    fs::create_dir_all(output_dir).map_err(|error| {
        format!(
            "could not create report directory {}: {error}",
            output_dir.display()
        )
    })?;
    let pair_results_jsonl = output_dir.join("pair-results.jsonl");
    crate::write_pair_results_jsonl(&pair_results_jsonl, results)?;
    let metrics_json = output_dir.join("metrics.json");
    let metrics = serde_json::to_string_pretty(report)
        .map_err(|error| format!("could not serialize metrics report: {error}"))?;
    fs::write(&metrics_json, format!("{metrics}\n")).map_err(|error| {
        format!(
            "could not write metrics {}: {error}",
            metrics_json.display()
        )
    })?;
    let metrics_summary = output_dir.join("metrics-summary.md");
    fs::write(&metrics_summary, metrics_summary_markdown(report)).map_err(|error| {
        format!(
            "could not write metrics summary {}: {error}",
            metrics_summary.display()
        )
    })?;
    let error_analysis = output_dir.join("error-analysis.md");
    fs::write(
        &error_analysis,
        error_analysis_markdown(results, report.production_operating_point_threshold),
    )
    .map_err(|error| {
        format!(
            "could not write error analysis {}: {error}",
            error_analysis.display()
        )
    })?;
    Ok(GeneratedReportArtifacts {
        pair_results_jsonl,
        metrics_json,
        metrics_summary,
        error_analysis,
    })
}

pub fn binary_metrics(results: &[PairResult], threshold: u32) -> BinaryMetrics {
    let mut true_positive = 0;
    let mut false_positive = 0;
    let mut false_negative = 0;
    let mut true_negative = 0;
    for result in results
        .iter()
        .filter(|result| result.gold_label.is_binary())
    {
        let predicted_relevant = result.match_strength >= threshold;
        match (result.gold_label.is_relevant(), predicted_relevant) {
            (true, true) => true_positive += 1,
            (false, true) => false_positive += 1,
            (true, false) => false_negative += 1,
            (false, false) => true_negative += 1,
        }
    }
    let precision = ratio(true_positive, true_positive + false_positive);
    let recall = ratio(true_positive, true_positive + false_negative);
    let f1 = match (precision, recall) {
        (Some(Ratio(precision)), Some(Ratio(recall))) if precision + recall > 0.0 => {
            Some(Ratio(2.0 * precision * recall / (precision + recall)))
        }
        _ => None,
    };
    BinaryMetrics {
        threshold,
        true_positive,
        false_positive,
        false_negative,
        true_negative,
        precision,
        recall,
        f1,
    }
}

fn paraphrase_cluster_recall(
    results: &[PairResult],
    threshold: u32,
) -> Vec<ParaphraseClusterRecall> {
    let mut clusters: BTreeMap<String, Vec<&PairResult>> = BTreeMap::new();
    for result in results {
        for tag in &result.slice_tags {
            if let Some(id) = tag.paraphrase_cluster_id() {
                clusters.entry(id.to_string()).or_default().push(result);
            }
        }
    }
    clusters
        .into_iter()
        .map(|(cluster_id, members)| {
            let relevant = members
                .into_iter()
                .filter(|result| result.gold_label == GoldLabel::Relevant)
                .collect::<Vec<_>>();
            let recalled = relevant
                .iter()
                .filter(|result| result.match_strength >= threshold)
                .count() as u32;
            let count = relevant.len() as u32;
            ParaphraseClusterRecall {
                cluster_id,
                relevant_pair_count: count,
                recalled_relevant_pair_count: recalled,
                recall: ratio(recalled, count),
            }
        })
        .collect()
}

fn required_and_observed_slices(results: &[PairResult]) -> Vec<String> {
    let mut names = BTreeSet::from([
        "asr_corruption".to_string(),
        "hypothetical".to_string(),
        "negation".to_string(),
        "quoted_speech".to_string(),
    ]);
    for result in results {
        for tag in &result.slice_tags {
            names.extend(tag.report_names().into_iter().map(ToString::to_string));
        }
    }
    names.into_iter().collect()
}

fn belongs_to_report_slice(result: &PairResult, slice_name: &str) -> bool {
    result
        .slice_tags
        .iter()
        .flat_map(|tag| tag.report_names())
        .any(|name| name == slice_name)
}

fn ratio(numerator: u32, denominator: u32) -> Option<Ratio> {
    (denominator > 0).then(|| Ratio(numerator as f64 / denominator as f64))
}

pub fn metrics_summary_markdown(report: &MetricsReport) -> String {
    let metrics = &report.production_operating_point;
    format!(
        "# Goal relevance baseline metrics\n\nDataset: `{}`  \nRoster snapshot: `{}`  \nProduction operating point: match strength ≥ {}\n\n| Metric | Value |\n| --- | ---: |\n| Precision | {} |\n| Recall | {} |\n| F1 | {} |\n| Ambiguous pairs excluded from binary metrics | {} |\n| NoneOfRoster negative pairs | {} |\n\nLatency is deferred: {}\n",
        report.dataset_version,
        report.roster_snapshot_hash,
        report.production_operating_point_threshold,
        display_ratio(metrics.precision),
        display_ratio(metrics.recall),
        display_ratio(metrics.f1),
        report.ambiguous_pair_count,
        report.none_of_roster_pair_count,
        report.latency.measurement_method
    )
}

pub fn error_analysis_markdown(results: &[PairResult], threshold: u32) -> String {
    let false_negatives = results
        .iter()
        .filter(|result| {
            result.gold_label == GoldLabel::Relevant && result.match_strength < threshold
        })
        .collect::<Vec<_>>();
    let false_positives = results
        .iter()
        .filter(|result| {
            result.gold_label == GoldLabel::NotRelevant && result.match_strength >= threshold
        })
        .collect::<Vec<_>>();
    format!(
        "# Goal relevance error analysis\n\nOperating point: match strength ≥ {threshold}.\n\n## Worst false negatives\n\n{}\n## False positives\n\n{}",
        grouped_errors(false_negatives, false),
        grouped_errors(false_positives, true)
    )
}

fn grouped_errors(mut results: Vec<&PairResult>, descending: bool) -> String {
    results.sort_by_key(|result| result.match_strength);
    if descending {
        results.reverse();
    }
    if results.is_empty() {
        return "None.\n".to_string();
    }
    let mut groups: BTreeMap<String, Vec<&PairResult>> = BTreeMap::new();
    for result in results {
        let names = result
            .slice_tags
            .iter()
            .flat_map(|tag| tag.report_names())
            .collect::<Vec<_>>();
        if names.is_empty() {
            groups
                .entry("untagged".to_string())
                .or_default()
                .push(result);
        } else {
            for name in names {
                groups.entry(name.to_string()).or_default().push(result);
            }
        }
    }
    groups
        .into_iter()
        .map(|(slice, members)| {
            let rows = members
                .into_iter()
                .map(|result| {
                    format!(
                        "- strength {} — {}",
                        result.match_strength, result.utterance
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("### {slice}\n\n{rows}\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn display_ratio(value: Option<Ratio>) -> String {
    value.map_or_else(|| "n/a".to_string(), |Ratio(value)| format!("{value:.3}"))
}

pub fn count_none_of_roster_pairs(dataset: &crate::Dataset) -> u32 {
    dataset
        .records
        .iter()
        .filter(|record| {
            record.utterance_roster_annotation == crate::UtteranceRosterAnnotation::NoneOfRoster
        })
        .count() as u32
}

pub fn build_report_for_dataset(
    results: &[PairResult],
    dataset: &crate::Dataset,
) -> Result<MetricsReport, String> {
    build_metrics_report(results, count_none_of_roster_pairs(dataset))
}

pub fn default_run_output_dir() -> PathBuf {
    run_output_dir_at(OffsetDateTime::now_utc())
}

pub(crate) fn run_output_dir_at(timestamp: OffsetDateTime) -> PathBuf {
    Path::new("runs").join(format!(
        "{:04}-{:02}-{:02}-{:02}{:02}{:02}-goal-relevance-sample",
        timestamp.year(),
        u8::from(timestamp.month()),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second()
    ))
}
