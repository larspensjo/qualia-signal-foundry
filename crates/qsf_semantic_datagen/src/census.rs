//! Deterministic census evidence over a lineage pool.
//!
//! The calculations in this module are pure.  The command wrapper deliberately only locates
//! lineage files and writes the resulting strict JSON artifact.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use qsf_semantic_eval::{Dataset, GoldLabel, PairRecord, RosterSnapshot, SliceTag};
use serde::{Deserialize, Serialize};

use crate::{
    GenerationOutput, LabelInterchange, ReviewedPoolRequest, Split, fold_reviewed_pool,
    parse_generation_output, parse_label_interchange, parse_split_summary, split_reviewed_pool,
};

/// The only allowed interpretation of the rubric-sensitivity measurement.
pub const RUBRIC_SENSITIVITY_USE: &str = "The rubric-sensitivity recall gap sizes a disclosure and identifies kinds of cases needing fresh worked examples. It must never inform the direction of the relevance threshold.";
pub const CONTESTED_TARGET: u32 = 60;
pub const STRICT_TARGET: u32 = 40;
pub const MINIMUM_POOLED_SAMPLE: u32 = 5;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportCell {
    pub goal_ref: String,
    pub split: Split,
    pub strict_count: u32,
    pub optimistic_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SliceSupportCell {
    pub slice: String,
    pub split: Split,
    pub strict_count: u32,
    pub optimistic_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportEvidence {
    pub evidence_version: u16,
    pub mode: String,
    pub pool_id: String,
    pub census_date: String,
    pub split_seed: u64,
    pub agreed_pairs: u32,
    pub disagreement_pairs: u32,
    pub total_pairs: u32,
    pub goal_split_cells: Vec<SupportCell>,
    pub slice_split_cells: Vec<SliceSupportCell>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallSummary {
    pub relevant_pairs: u32,
    pub matched_relevant_pairs: u32,
    pub recall_bp: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RubricSensitivityEvidence {
    pub evidence_version: u16,
    pub mode: String,
    pub pool_id: String,
    pub census_date: String,
    pub scorer_source: String,
    pub strict: RecallSummary,
    pub optimistic: RecallSummary,
    pub recall_gap_bp: i32,
    pub permitted_uses: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SampleStratum {
    pub goal_ref: String,
    pub split: Split,
    pub population: u32,
    pub allocation: u32,
    pub drawn_pair_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SampleEvidence {
    pub evidence_version: u16,
    pub mode: String,
    pub pool_id: String,
    pub census_date: String,
    pub split_seed: u64,
    pub sample_seed: u64,
    pub population_kind: String,
    pub samples: Vec<SampleDraw>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SampleDraw {
    pub sample_name: String,
    pub target_size: u32,
    pub realised_size: u32,
    pub strata: Vec<SampleStratum>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionObservation {
    pub pair_id: String,
    pub goal_ref: String,
    pub split: Split,
    pub unanimous_relevant: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TiePartition {
    pub goal_ref: String,
    pub split: Split,
    pub slice: String,
    pub count: u32,
}

/// The handoff shape consumed by `census recensus`; it contains counts, never tie rates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecensusInput {
    pub strict_observations: Vec<ProjectionObservation>,
    pub contested_observations: Vec<ProjectionObservation>,
    pub tie_partitions: Vec<TiePartition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCell {
    pub goal_ref: String,
    pub split: Split,
    pub strict_count: u32,
    pub contested_count: u32,
    pub strict_sample_count: u32,
    pub contested_sample_count: u32,
    pub s_bp: Option<u32>,
    pub p_bp: Option<u32>,
    pub strict_fallback: String,
    pub contested_fallback: String,
    pub strict_term: Option<u32>,
    pub contested_term: Option<u32>,
    pub projected_support: Option<u32>,
    pub clears_support_margin: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecensusEvidence {
    pub evidence_version: u16,
    pub mode: String,
    pub pool_id: String,
    pub census_date: String,
    pub min_relevant_support: u32,
    pub support_margin: u32,
    pub cells: Vec<ProjectionCell>,
    pub tie_partitions: Vec<TiePartition>,
}

#[derive(Clone, Debug)]
pub(crate) struct PairView {
    id: String,
    utterance_id: String,
    goal_ref: String,
    split: Split,
    mini: GoldLabel,
    fable: GoldLabel,
    slices: Vec<String>,
}

pub fn parse_support_evidence(input: &str) -> Result<SupportEvidence, String> {
    serde_json::from_str(input).map_err(|error| format!("invalid census support evidence: {error}"))
}
pub fn parse_rubric_sensitivity_evidence(input: &str) -> Result<RubricSensitivityEvidence, String> {
    serde_json::from_str(input)
        .map_err(|error| format!("invalid census rubric-sensitivity evidence: {error}"))
}
pub fn parse_sample_evidence(input: &str) -> Result<SampleEvidence, String> {
    serde_json::from_str(input).map_err(|error| format!("invalid census sample evidence: {error}"))
}
pub fn parse_recensus_evidence(input: &str) -> Result<RecensusEvidence, String> {
    serde_json::from_str(input)
        .map_err(|error| format!("invalid census recensus evidence: {error}"))
}
pub fn parse_recensus_input(input: &str) -> Result<RecensusInput, String> {
    serde_json::from_str(input).map_err(|error| format!("invalid census recensus input: {error}"))
}

pub(crate) fn census_support(
    pool_id: &str,
    census_date: &str,
    split_seed: u64,
    views: &[PairView],
) -> Result<SupportEvidence, String> {
    let evidence = census_support_unchecked(pool_id, census_date, split_seed, views)?;
    if evidence.total_pairs != 798
        || evidence.agreed_pairs != 591
        || evidence.disagreement_pairs != 207
    {
        return Err(format!(
            "census integrity check failed: expected 591/798 agreement and 207 disagreements, found {}/{} agreement and {} disagreements",
            evidence.agreed_pairs, evidence.total_pairs, evidence.disagreement_pairs
        ));
    }
    Ok(evidence)
}

fn census_support_unchecked(
    pool_id: &str,
    census_date: &str,
    split_seed: u64,
    views: &[PairView],
) -> Result<SupportEvidence, String> {
    let mut goal_cells: BTreeMap<(String, Split), (u32, u32)> = BTreeMap::new();
    let mut slice_cells: BTreeMap<(String, Split), (u32, u32)> = BTreeMap::new();
    let mut agreed = 0;
    for view in views {
        if view.mini == view.fable {
            agreed += 1;
        }
        let strict = (view.mini.is_relevant() && view.fable.is_relevant()) as u32;
        let optimistic = (view.mini.is_relevant() || view.fable.is_relevant()) as u32;
        let counts = goal_cells
            .entry((view.goal_ref.clone(), view.split))
            .or_default();
        counts.0 += strict;
        counts.1 += optimistic;
        for slice in &view.slices {
            let counts = slice_cells.entry((slice.clone(), view.split)).or_default();
            counts.0 += strict;
            counts.1 += optimistic;
        }
    }
    let total_pairs =
        u32::try_from(views.len()).map_err(|_| "too many census pairs".to_string())?;
    Ok(SupportEvidence {
        evidence_version: 1,
        mode: "support".to_string(),
        pool_id: pool_id.to_string(),
        census_date: census_date.to_string(),
        split_seed,
        agreed_pairs: agreed,
        disagreement_pairs: total_pairs - agreed,
        total_pairs,
        goal_split_cells: goal_cells
            .into_iter()
            .map(
                |((goal_ref, split), (strict_count, optimistic_count))| SupportCell {
                    goal_ref,
                    split,
                    strict_count,
                    optimistic_count,
                },
            )
            .collect(),
        slice_split_cells: slice_cells
            .into_iter()
            .map(
                |((slice, split), (strict_count, optimistic_count))| SliceSupportCell {
                    slice,
                    split,
                    strict_count,
                    optimistic_count,
                },
            )
            .collect(),
    })
}

pub(crate) fn census_rubric_sensitivity(
    pool_id: &str,
    census_date: &str,
    roster: &RosterSnapshot,
    generated: &[GenerationOutput],
    mini: &[LabelInterchange],
    fable: &[LabelInterchange],
    views: &[PairView],
) -> Result<RubricSensitivityEvidence, String> {
    let strict_records = proxy_records(generated, roster, mini, fable, views, |view| {
        view.mini.is_relevant() && view.fable.is_relevant()
    })?;
    let optimistic_records = proxy_records(generated, roster, mini, fable, views, |view| {
        view.mini.is_relevant() || view.fable.is_relevant()
    })?;
    let strict = recall_summary(&strict_records, roster)?;
    let optimistic = recall_summary(&optimistic_records, roster)?;
    Ok(RubricSensitivityEvidence {
        evidence_version: 1,
        mode: "rubric-sensitivity".to_string(),
        pool_id: pool_id.to_string(),
        census_date: census_date.to_string(),
        scorer_source: qsf_semantic_eval::SCORER_SOURCE.to_string(),
        recall_gap_bp: i32::try_from(optimistic.recall_bp).unwrap_or_default()
            - i32::try_from(strict.recall_bp).unwrap_or_default(),
        strict,
        optimistic,
        permitted_uses: RUBRIC_SENSITIVITY_USE.to_string(),
    })
}

fn recall_summary(
    records: &[PairRecord],
    roster: &RosterSnapshot,
) -> Result<RecallSummary, String> {
    let results = qsf_semantic_eval::run_baseline(
        &Dataset {
            records: records.to_vec(),
        },
        roster,
    )?;
    let relevant_pairs = results
        .iter()
        .filter(|item| item.gold_label.is_relevant())
        .count() as u32;
    let matched_relevant_pairs = results
        .iter()
        .filter(|item| item.gold_label.is_relevant() && item.qualifies_at_threshold_in_force)
        .count() as u32;
    Ok(RecallSummary {
        relevant_pairs,
        matched_relevant_pairs,
        recall_bp: bp(matched_relevant_pairs, relevant_pairs),
    })
}

fn proxy_records(
    generated: &[GenerationOutput],
    roster: &RosterSnapshot,
    mini: &[LabelInterchange],
    _fable: &[LabelInterchange],
    views: &[PairView],
    choose: impl Fn(&PairView) -> bool,
) -> Result<Vec<PairRecord>, String> {
    let labels: BTreeMap<_, _> = views
        .iter()
        .map(|view| {
            (
                (view.utterance_id.clone(), view.goal_ref.clone()),
                if choose(view) {
                    GoldLabel::Relevant
                } else {
                    GoldLabel::NotRelevant
                },
            )
        })
        .collect();
    let mut proxy = mini.to_vec();
    for interchange in &mut proxy {
        for per_goal in &mut interchange.per_goal {
            per_goal.label = labels
                .get(&(interchange.utterance_id.clone(), per_goal.goal_ref.clone()))
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "proxy construction missing pair {}:{}",
                        interchange.utterance_id, per_goal.goal_ref
                    )
                })?;
        }
        interchange.none_of_roster = !interchange
            .per_goal
            .iter()
            .any(|item| item.label.is_relevant());
    }
    // The existing fold is the single PairRecord constructor.  Passing the proxy through both
    // labeler slots makes its selected label the fold's ordinary initial value.
    fold_reviewed_pool(ReviewedPoolRequest {
        generated,
        roster,
        mini: &proxy,
        fable: &proxy,
        decisions: &[],
        dataset_version: "goalrel-v1-proxy",
        generation_output_sha256: "proxy",
        label_mini_sha256: "proxy",
        label_fable_sha256: "proxy",
        review_decisions_sha256: "proxy",
    })
}

pub(crate) fn census_sample(
    pool_id: &str,
    census_date: &str,
    split_seed: u64,
    sample_seed: u64,
    views: &[PairView],
    population_kind: &str,
    target_size: u32,
) -> Result<SampleEvidence, String> {
    let selected = views
        .iter()
        .filter(|view| match population_kind {
            "contested" => view.mini.is_relevant() && !view.fable.is_relevant(),
            "strict" => view.mini.is_relevant() && view.fable.is_relevant(),
            _ => false,
        })
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(format!(
            "unknown or empty census sample population {population_kind}"
        ));
    }
    let sample_a = sample_draw("a", population_kind, sample_seed, &selected, target_size)?;
    let drawn = sample_a
        .strata
        .iter()
        .flat_map(|item| item.drawn_pair_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let remaining = selected
        .into_iter()
        .filter(|view| !drawn.contains(&view.id))
        .collect::<Vec<_>>();
    let samples = if population_kind == "contested" {
        vec![
            sample_a,
            sample_draw("b", population_kind, sample_seed, &remaining, target_size)?,
        ]
    } else {
        vec![SampleDraw {
            sample_name: "strict".to_string(),
            ..sample_a
        }]
    };
    Ok(SampleEvidence {
        evidence_version: 1,
        mode: format!("sample-{population_kind}"),
        pool_id: pool_id.to_string(),
        census_date: census_date.to_string(),
        split_seed,
        sample_seed,
        population_kind: population_kind.to_string(),
        samples,
    })
}

fn sample_draw(
    name: &str,
    population_kind: &str,
    sample_seed: u64,
    population: &[PairView],
    target_size: u32,
) -> Result<SampleDraw, String> {
    let mut strata: BTreeMap<(String, Split), Vec<&PairView>> = BTreeMap::new();
    for view in population {
        strata
            .entry((view.goal_ref.clone(), view.split))
            .or_default()
            .push(view);
    }
    let allocations = allocate_strata(&strata, target_size)?;
    let mut evidence_strata = Vec::new();
    for ((goal_ref, split), members) in strata {
        let allocation = *allocations
            .get(&(goal_ref.clone(), split))
            .expect("allocated stratum");
        let population_count = members.len() as u32;
        let mut ordered = members;
        ordered.sort_by_key(|view| {
            seeded_key(
                sample_seed,
                &format!("{population_kind}:{name}:{}", view.id),
            )
        });
        let drawn_pair_ids = ordered
            .into_iter()
            .take(allocation as usize)
            .map(|view| view.id.clone())
            .collect();
        evidence_strata.push(SampleStratum {
            goal_ref,
            split,
            population: population_count,
            allocation,
            drawn_pair_ids,
        });
    }
    let realised_size = evidence_strata.iter().map(|item| item.allocation).sum();
    Ok(SampleDraw {
        sample_name: name.to_string(),
        target_size,
        realised_size,
        strata: evidence_strata,
    })
}

/// Shared strict/contested allocation path: proportional largest remainder with a non-empty floor.
fn allocate_strata<T>(
    strata: &BTreeMap<(String, Split), Vec<T>>,
    target: u32,
) -> Result<BTreeMap<(String, Split), u32>, String> {
    let total: u32 = strata
        .values()
        .map(|items| u32::try_from(items.len()).unwrap_or(u32::MAX))
        .sum();
    if total == 0 {
        return Err("cannot sample an empty census population".to_string());
    }
    let mut allocations = BTreeMap::new();
    let mut floors = 0;
    for (key, items) in strata {
        let floor = u32::try_from(items.len()).unwrap_or(u32::MAX).min(3);
        floors += floor;
        allocations.insert(key.clone(), floor);
    }
    let realised_target = target.max(floors).min(total);
    let remaining_after_floors = realised_target - floors;
    let mut ranked = Vec::new();
    for (key, items) in strata {
        let size = u32::try_from(items.len()).unwrap_or(u32::MAX);
        let proportional = size.saturating_mul(remaining_after_floors) / total;
        let remainder = size.saturating_mul(remaining_after_floors) % total;
        let current = allocations.get(key).copied().unwrap_or_default();
        let desired = current.saturating_add(proportional).min(size);
        allocations.insert(key.clone(), desired);
        ranked.push((remainder, key.clone()));
    }
    let allocated: u32 = allocations.values().sum();
    let mut remaining = realised_target.saturating_sub(allocated);
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    while remaining > 0 {
        let mut progressed = false;
        for (_, key) in &ranked {
            let max = u32::try_from(strata.get(key).expect("stratum").len()).unwrap_or(u32::MAX);
            let allocation = allocations.get_mut(key).expect("allocation");
            if *allocation < max {
                *allocation += 1;
                remaining -= 1;
                progressed = true;
                if remaining == 0 {
                    break;
                }
            }
        }
        if !progressed {
            return Err("could not complete stratified allocation".to_string());
        }
    }
    Ok(allocations)
}

pub fn census_recensus(
    pool_id: &str,
    census_date: &str,
    support: &SupportEvidence,
    input: &RecensusInput,
    min_relevant_support: u32,
) -> RecensusEvidence {
    let mut strict_by_cell: BTreeMap<(String, Split), Vec<&ProjectionObservation>> =
        BTreeMap::new();
    let mut contested_by_cell: BTreeMap<(String, Split), Vec<&ProjectionObservation>> =
        BTreeMap::new();
    for item in &input.strict_observations {
        strict_by_cell
            .entry((item.goal_ref.clone(), item.split))
            .or_default()
            .push(item);
    }
    for item in &input.contested_observations {
        contested_by_cell
            .entry((item.goal_ref.clone(), item.split))
            .or_default()
            .push(item);
    }
    let mut cells = Vec::new();
    for support_cell in &support.goal_split_cells {
        let key = (support_cell.goal_ref.clone(), support_cell.split);
        let strict = strict_by_cell.get(&key).cloned().unwrap_or_default();
        let contested = contested_by_cell.get(&key).cloned().unwrap_or_default();
        let (s_bp, strict_fallback) =
            projected_bp(&strict_by_cell, &support_cell.goal_ref, support_cell.split);
        let (p_bp, contested_fallback) = projected_bp(
            &contested_by_cell,
            &support_cell.goal_ref,
            support_cell.split,
        );
        let strict_term = if support_cell.strict_count == 0 {
            Some(0)
        } else {
            s_bp.map(|value| value.saturating_mul(support_cell.strict_count) / 10000)
        };
        let contested_count = support_cell
            .optimistic_count
            .saturating_sub(support_cell.strict_count);
        let contested_term = if contested_count == 0 {
            Some(0)
        } else {
            p_bp.map(|value| value.saturating_mul(contested_count) / 10000)
        };
        let projected_support = strict_term.zip(contested_term).map(|(a, b)| a + b);
        cells.push(ProjectionCell {
            goal_ref: support_cell.goal_ref.clone(),
            split: support_cell.split,
            strict_count: support_cell.strict_count,
            contested_count,
            strict_sample_count: strict.len() as u32,
            contested_sample_count: contested.len() as u32,
            s_bp,
            p_bp,
            strict_fallback,
            contested_fallback,
            strict_term,
            contested_term,
            clears_support_margin: projected_support
                .is_some_and(|value| value >= min_relevant_support + 2),
            projected_support,
        });
    }
    RecensusEvidence {
        evidence_version: 1,
        mode: "recensus".to_string(),
        pool_id: pool_id.to_string(),
        census_date: census_date.to_string(),
        min_relevant_support,
        support_margin: 2,
        cells,
        tie_partitions: input.tie_partitions.clone(),
    }
}

fn projected_bp(
    by_cell: &BTreeMap<(String, Split), Vec<&ProjectionObservation>>,
    goal_ref: &str,
    split: Split,
) -> (Option<u32>, String) {
    let direct = by_cell
        .get(&(goal_ref.to_string(), split))
        .cloned()
        .unwrap_or_default();
    if direct.len() >= MINIMUM_POOLED_SAMPLE as usize {
        return (
            Some(bp(
                direct.iter().filter(|item| item.unanimous_relevant).count() as u32,
                direct.len() as u32,
            )),
            "cell".to_string(),
        );
    }
    let pooled = [Split::Validation, Split::Test]
        .into_iter()
        .flat_map(|candidate| {
            by_cell
                .get(&(goal_ref.to_string(), candidate))
                .into_iter()
                .flatten()
                .copied()
        })
        .collect::<Vec<_>>();
    if pooled.len() >= MINIMUM_POOLED_SAMPLE as usize {
        (
            Some(bp(
                pooled.iter().filter(|item| item.unanimous_relevant).count() as u32,
                pooled.len() as u32,
            )),
            "goal_pooled".to_string(),
        )
    } else {
        (None, "unprojectable".to_string())
    }
}

fn bp(numerator: u32, denominator: u32) -> u32 {
    numerator
        .saturating_mul(10000)
        .checked_div(denominator)
        .unwrap_or_default()
}

fn seeded_key(seed: u64, value: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(seed.to_le_bytes());
    hasher.update(value.as_bytes());
    hasher.finalize().into()
}

pub fn run_census_cli(args: &[String]) -> Result<(), String> {
    let (lineage_root, positionals) = parse_root(args)?;
    if positionals.len() < 4 {
        return Err(census_usage().to_string());
    }
    let mode = positionals[0].as_str();
    let roster = RosterSnapshot::from_json_path(&positionals[1])?;
    let pool_id = &positionals[2];
    let date = &positionals[3];
    let loaded = load_pool(&lineage_root, pool_id, &roster)?;
    let output = match mode {
        "support" if positionals.len() == 4 => serde_json::to_string_pretty(&census_support(
            pool_id,
            date,
            loaded.split_seed,
            &loaded.views,
        )?),
        "rubric-sensitivity" if positionals.len() == 4 => {
            serde_json::to_string_pretty(&census_rubric_sensitivity(
                pool_id,
                date,
                &roster,
                &loaded.generated,
                &loaded.mini,
                &loaded.fable,
                &loaded.views,
            )?)
        }
        "sample-contested" if positionals.len() == 5 => {
            serde_json::to_string_pretty(&census_sample(
                pool_id,
                date,
                loaded.split_seed,
                positionals[4]
                    .parse()
                    .map_err(|e| format!("sample seed must be unsigned: {e}"))?,
                &loaded.views,
                "contested",
                CONTESTED_TARGET,
            )?)
        }
        "sample-strict" if positionals.len() == 5 => serde_json::to_string_pretty(&census_sample(
            pool_id,
            date,
            loaded.split_seed,
            positionals[4]
                .parse()
                .map_err(|e| format!("sample seed must be unsigned: {e}"))?,
            &loaded.views,
            "strict",
            STRICT_TARGET,
        )?),
        "recensus" if positionals.len() == 6 => {
            let support = census_support(pool_id, date, loaded.split_seed, &loaded.views)?;
            let input =
                parse_recensus_input(&fs::read_to_string(&positionals[4]).map_err(|e| {
                    format!("could not read recensus input {}: {e}", positionals[4])
                })?)?;
            let floor = positionals[5]
                .parse()
                .map_err(|e| format!("min relevant support must be unsigned: {e}"))?;
            serde_json::to_string_pretty(&census_recensus(pool_id, date, &support, &input, floor))
        }
        _ => return Err(census_usage().to_string()),
    }
    .map_err(|error| format!("could not serialize census evidence: {error}"))?
        + "\n";
    let path = lineage_root
        .join("pools")
        .join(pool_id)
        .join("evidence")
        .join("census")
        .join(format!("{date}-{mode}.json"));
    fs::create_dir_all(path.parent().expect("evidence parent")).map_err(|error| {
        format!(
            "could not create census evidence directory {}: {error}",
            path.display()
        )
    })?;
    fs::write(&path, output).map_err(|error| {
        format!(
            "could not write census evidence {}: {error}",
            path.display()
        )
    })?;
    engine_logging::engine_info!(
        "wrote goal relevance census evidence mode={} pool_id={} path={}",
        mode,
        pool_id,
        path.display()
    );
    Ok(())
}

fn census_usage() -> &'static str {
    "census <support|rubric-sensitivity|sample-contested|sample-strict|recensus> <roster.json> <pool-id> <date> [sample-seed | <recensus-input.json> <min-relevant-support>] [--lineage-root <path>]"
}
fn parse_root(args: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    let mut root = PathBuf::from("evaluation/frozen/goal-relevance/lineage");
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--lineage-root" {
            index += 1;
            root = PathBuf::from(args.get(index).ok_or_else(|| census_usage().to_string())?);
        } else if args[index].starts_with("--") {
            return Err(census_usage().to_string());
        } else {
            positionals.push(args[index].clone());
        }
        index += 1;
    }
    Ok((root, positionals))
}

struct LoadedPool {
    generated: Vec<GenerationOutput>,
    mini: Vec<LabelInterchange>,
    fable: Vec<LabelInterchange>,
    split_seed: u64,
    views: Vec<PairView>,
}
fn load_pool(root: &Path, pool_id: &str, roster: &RosterSnapshot) -> Result<LoadedPool, String> {
    let pool = root.join("pools").join(pool_id);
    let generated = parse_generation_output(&read(&pool.join("generation-output.jsonl"))?)?;
    let mini = parse_label_interchange(
        &read(&pool.join("evidence/goalrel-label-v1/label-mini.jsonl"))?,
        roster,
    )?;
    let fable = parse_label_interchange(
        &read(&pool.join("evidence/goalrel-label-v1/label-fable.jsonl"))?,
        roster,
    )?;
    let summary = parse_split_summary(&read(&pool.join("split-summary.json"))?)?;
    let proxies = proxy_records(
        &generated,
        roster,
        &mini,
        &fable,
        &label_views(&generated, &mini, &fable, &BTreeMap::new())?,
        |_| false,
    )?;
    let split = split_reviewed_pool(&proxies, summary.split_seed)?;
    if split.assignment_by_component != summary.assignment_by_component {
        return Err(
            "census split binding does not reproduce split-summary assignment_by_component"
                .to_string(),
        );
    }
    let by_id: BTreeMap<_, _> = split
        .validation
        .iter()
        .map(|item| (item.utterance_id.clone(), Split::Validation))
        .chain(
            split
                .test
                .iter()
                .map(|item| (item.utterance_id.clone(), Split::Test)),
        )
        .collect();
    let views = label_views(&generated, &mini, &fable, &by_id)?;
    Ok(LoadedPool {
        generated,
        mini,
        fable,
        split_seed: summary.split_seed,
        views,
    })
}
fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("could not read census input {}: {error}", path.display()))
}
fn label_views(
    generated: &[GenerationOutput],
    mini: &[LabelInterchange],
    fable: &[LabelInterchange],
    splits: &BTreeMap<String, Split>,
) -> Result<Vec<PairView>, String> {
    let generated = generated
        .iter()
        .map(|item| (item.utterance_id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let labels = |items: &[LabelInterchange]| {
        items
            .iter()
            .flat_map(|item| {
                item.per_goal.iter().map(move |goal| {
                    (
                        (item.utterance_id.clone(), goal.goal_ref.clone()),
                        goal.label.clone(),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>()
    };
    let mini_labels = labels(mini);
    let fable_labels = labels(fable);
    mini_labels
        .into_iter()
        .map(|((utterance_id, goal_ref), mini)| {
            let generated = generated.get(&utterance_id).ok_or_else(|| {
                format!("census label references unknown utterance {utterance_id}")
            })?;
            let split = splits
                .get(&utterance_id)
                .copied()
                .unwrap_or(Split::Validation);
            Ok(PairView {
                id: format!("{utterance_id}:{goal_ref}"),
                utterance_id,
                goal_ref: goal_ref.clone(),
                split,
                mini,
                fable: fable_labels
                    .get(&(generated.utterance_id.clone(), goal_ref))
                    .cloned()
                    .ok_or_else(|| "census label sets do not have the same pairs".to_string())?,
                slices: generated
                    .intended_slice_tags
                    .iter()
                    .flat_map(SliceTag::report_names)
                    .map(str::to_string)
                    .collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: &str, goal: &str, split: Split, mini: GoldLabel, fable: GoldLabel) -> PairView {
        PairView {
            id: id.to_string(),
            utterance_id: format!("u-{id}"),
            goal_ref: goal.to_string(),
            split,
            mini,
            fable,
            slices: Vec::new(),
        }
    }

    #[test]
    fn sampled_populations_share_the_single_allocator_and_contested_draws_are_disjoint() {
        let mut views = Vec::new();
        for index in 0..8 {
            views.push(view(
                &format!("c{index}"),
                if index < 4 { "g1" } else { "g2" },
                if index < 4 {
                    Split::Validation
                } else {
                    Split::Test
                },
                GoldLabel::Relevant,
                GoldLabel::NotRelevant,
            ));
        }
        for index in 0..4 {
            views.push(view(
                &format!("s{index}"),
                if index < 2 { "g1" } else { "g2" },
                if index < 2 {
                    Split::Validation
                } else {
                    Split::Test
                },
                GoldLabel::Relevant,
                GoldLabel::Relevant,
            ));
        }
        let contested = census_sample("pool", "20260726", 7, 11, &views, "contested", 2)
            .expect("contested sample");
        let strict =
            census_sample("pool", "20260726", 7, 11, &views, "strict", 2).expect("strict sample");
        // Both calls exercise census_sample -> sample_draw -> allocate_strata; this shared assertion
        // pins the common allocator's floor behaviour for the two distinct populations.
        assert_eq!(
            contested.samples[0]
                .strata
                .iter()
                .map(|item| item.allocation)
                .sum::<u32>(),
            6
        );
        assert_eq!(
            strict.samples[0]
                .strata
                .iter()
                .map(|item| item.allocation)
                .sum::<u32>(),
            4
        );
        let a = contested.samples[0]
            .strata
            .iter()
            .flat_map(|item| item.drawn_pair_ids.iter())
            .collect::<BTreeSet<_>>();
        let b = contested.samples[1]
            .strata
            .iter()
            .flat_map(|item| item.drawn_pair_ids.iter())
            .collect::<BTreeSet<_>>();
        assert!(a.is_disjoint(&b));
    }

    #[test]
    fn support_cells_are_hand_computable_on_a_small_pool() {
        let views = vec![
            view(
                "a",
                "g1",
                Split::Validation,
                GoldLabel::Relevant,
                GoldLabel::Relevant,
            ),
            view(
                "b",
                "g1",
                Split::Validation,
                GoldLabel::Relevant,
                GoldLabel::NotRelevant,
            ),
            view(
                "c",
                "g2",
                Split::Test,
                GoldLabel::NotRelevant,
                GoldLabel::Relevant,
            ),
        ];
        let evidence =
            census_support_unchecked("fixture", "20260726", 1, &views).expect("small census");
        assert_eq!(evidence.agreed_pairs, 1);
        assert_eq!(evidence.disagreement_pairs, 2);
        assert_eq!(
            evidence.goal_split_cells[0],
            SupportCell {
                goal_ref: "g1".to_string(),
                split: Split::Validation,
                strict_count: 1,
                optimistic_count: 2
            }
        );
        assert_eq!(
            evidence.goal_split_cells[1],
            SupportCell {
                goal_ref: "g2".to_string(),
                split: Split::Test,
                strict_count: 0,
                optimistic_count: 1
            }
        );
    }

    #[test]
    fn recensus_uses_no_tie_quantity_and_four_does_not_clear_three_with_margin() {
        let support = SupportEvidence {
            evidence_version: 1,
            mode: "support".to_string(),
            pool_id: "p".to_string(),
            census_date: "d".to_string(),
            split_seed: 1,
            agreed_pairs: 591,
            disagreement_pairs: 207,
            total_pairs: 798,
            goal_split_cells: vec![SupportCell {
                goal_ref: "g".to_string(),
                split: Split::Validation,
                strict_count: 4,
                optimistic_count: 4,
            }],
            slice_split_cells: Vec::new(),
        };
        let observations = (0..5)
            .map(|index| ProjectionObservation {
                pair_id: format!("p{index}"),
                goal_ref: "g".to_string(),
                split: Split::Validation,
                unanimous_relevant: true,
            })
            .collect();
        let evidence = census_recensus(
            "p",
            "d",
            &support,
            &RecensusInput {
                strict_observations: observations,
                contested_observations: Vec::new(),
                tie_partitions: vec![TiePartition {
                    goal_ref: "g".to_string(),
                    split: Split::Validation,
                    slice: "negation".to_string(),
                    count: 9,
                }],
            },
            3,
        );
        assert_eq!(evidence.cells[0].projected_support, Some(4));
        assert!(!evidence.cells[0].clears_support_margin);
        let json = serde_json::to_string(&evidence).expect("evidence serializes");
        assert!(json.contains("\"count\":9"));
        assert!(!json.contains("tie_rate"));
    }

    #[test]
    fn production_seed_reproduces_pool_a_validation_binding() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../evaluation/frozen/goal-relevance/lineage");
        let roster = RosterSnapshot::from_json_path(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json"),
        )
        .expect("roster");
        let pool = root.join("pools/goalrel-generation-live");
        let generated = parse_generation_output(
            &read(&pool.join("generation-output.jsonl")).expect("generation"),
        )
        .expect("parse generation");
        let mini = parse_label_interchange(
            &read(&pool.join("evidence/goalrel-label-v1/label-mini.jsonl")).expect("mini"),
            &roster,
        )
        .expect("parse mini");
        let fable = parse_label_interchange(
            &read(&pool.join("evidence/goalrel-label-v1/label-fable.jsonl")).expect("fable"),
            &roster,
        )
        .expect("parse fable");
        let views = label_views(&generated, &mini, &fable, &BTreeMap::new()).expect("views");
        let proxy =
            proxy_records(&generated, &roster, &mini, &fable, &views, |_| false).expect("proxy");
        let seed = (20260726..)
            .find(|seed| {
                split_reviewed_pool(&proxy, *seed)
                    .expect("split")
                    .validation
                    .iter()
                    .all(|record| record.utterance_id.contains("pool-a"))
            })
            .expect("date-shaped seed binds pool-a to validation");
        assert_eq!(
            seed, 20260736,
            "the first deterministic date-shaped seed is the committed binding"
        );
    }

    #[test]
    fn committed_inputs_reproduce_the_published_goal_split_census_and_generated_artifacts_parse() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../evaluation/frozen/goal-relevance/lineage");
        let roster = RosterSnapshot::from_json_path(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json"),
        )
        .expect("roster");
        let loaded =
            load_pool(&root, "goalrel-generation-live", &roster).expect("committed lineage");
        let support = census_support(
            "goalrel-generation-live",
            "2026-07-26",
            loaded.split_seed,
            &loaded.views,
        )
        .expect("integrity census");
        assert_eq!(
            (
                support.agreed_pairs,
                support.total_pairs,
                support.disagreement_pairs
            ),
            (591, 798, 207)
        );
        let expected_by_title = [
            ("Respect a person's boundaries", 20, 25, 23, 28),
            ("Keep theses distinct from fact", 8, 32, 8, 26),
            ("Serve the present person", 4, 13, 4, 20),
            ("Grow the library", 5, 33, 5, 33),
            ("Learn what drives this person", 4, 13, 4, 16),
            ("Track the AI transition", 5, 7, 5, 6),
            ("Assemble a world picture", 9, 34, 9, 31),
        ];
        for (title, validation_strict, validation_optimistic, test_strict, test_optimistic) in
            expected_by_title
        {
            let goal_ref = &roster
                .goals
                .iter()
                .find(|goal| goal.title == title)
                .expect("published goal")
                .goal_ref;
            assert!(support.goal_split_cells.contains(&SupportCell {
                goal_ref: goal_ref.clone(),
                split: Split::Validation,
                strict_count: validation_strict,
                optimistic_count: validation_optimistic,
            }));
            assert!(support.goal_split_cells.contains(&SupportCell {
                goal_ref: goal_ref.clone(),
                split: Split::Test,
                strict_count: test_strict,
                optimistic_count: test_optimistic,
            }));
        }
        let pool = root.join("pools/goalrel-generation-live");
        let summary =
            parse_split_summary(&read(&pool.join("split-summary.json")).expect("summary"))
                .expect("strict split summary parser");
        assert_eq!(summary.split_seed, 20260736);
        assert!(
            summary
                .assignment_rationale
                .contains("pool-a -> validation")
        );
        let evidence = pool.join("evidence/census");
        let parsed_support = parse_support_evidence(
            &read(&evidence.join("2026-07-26-support.json")).expect("support artifact"),
        )
        .expect("support parses");
        assert_eq!(parsed_support, support);
        let sensitivity = parse_rubric_sensitivity_evidence(
            &read(&evidence.join("2026-07-26-rubric-sensitivity.json"))
                .expect("sensitivity artifact"),
        )
        .expect("sensitivity parses");
        assert_eq!(sensitivity.permitted_uses, RUBRIC_SENSITIVITY_USE);
        assert!(sensitivity.recall_gap_bp < 0);
        for name in [
            "2026-07-26-sample-contested.json",
            "2026-07-26-sample-strict.json",
        ] {
            let sample =
                parse_sample_evidence(&read(&evidence.join(name)).expect("sample artifact"))
                    .expect("sample parses");
            assert!(
                sample
                    .samples
                    .iter()
                    .all(|draw| draw.strata.iter().all(|stratum| stratum.population
                        == stratum.drawn_pair_ids.len() as u32
                        || stratum.population > stratum.drawn_pair_ids.len() as u32))
            );
        }
    }
}
