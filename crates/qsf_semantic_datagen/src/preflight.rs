use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use qsf_semantic_eval::SliceTag;
use serde::{Deserialize, Serialize};

use crate::GenerationOutput;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Validation,
    Test,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitFeasibility {
    pub component_by_utterance_id: BTreeMap<String, String>,
    pub assignment_by_component: BTreeMap<String, Split>,
}

#[derive(Clone, Copy)]
struct Floor {
    name: &'static str,
    count: usize,
}

const FLOORS: &[Floor] = &[
    Floor {
        name: "negation",
        count: 6,
    },
    Floor {
        name: "explicit_negation",
        count: 2,
    },
    Floor {
        name: "implicit_negation",
        count: 2,
    },
    Floor {
        name: "quoted_speech",
        count: 5,
    },
    Floor {
        name: "hypothetical",
        count: 5,
    },
    Floor {
        name: "subject_confusion",
        count: 3,
    },
    Floor {
        name: "punctuation_casing_loss",
        count: 3,
    },
    Floor {
        name: "synthetic_asr",
        count: 3,
    },
    Floor {
        name: "rare_high_cost",
        count: 3,
    },
    Floor {
        name: "hard_negative",
        count: 4,
    },
    Floor {
        name: "none_of_roster",
        count: 8,
    },
    Floor {
        name: "paraphrase_clusters",
        count: 4,
    },
];

const FLOOR_COUNT: usize = FLOORS.len();

#[derive(Clone, Debug)]
struct ComponentCounts {
    name: String,
    counts: [usize; FLOOR_COUNT],
}

#[derive(Clone, Debug)]
struct FailedAssignment {
    counts: [[usize; FLOOR_COUNT]; 2],
    total_shortfall: usize,
    minimum_ratio_millionths: usize,
}

pub fn split_feasibility_preflight(
    records: &[GenerationOutput],
    seed: u64,
) -> Result<SplitFeasibility, String> {
    let component_by_utterance_id = connected_components(records)?;
    let mut records_by_component: BTreeMap<String, Vec<&GenerationOutput>> = BTreeMap::new();
    for record in records {
        let component = component_by_utterance_id
            .get(&record.utterance_id)
            .expect("component exists")
            .clone();
        records_by_component
            .entry(component)
            .or_default()
            .push(record);
    }
    let mut components: Vec<_> = records_by_component
        .into_iter()
        .map(|(name, records)| ComponentCounts {
            name,
            counts: counts_for_component(&records),
        })
        .collect();
    components.sort_by(|left, right| {
        component_constraint_weight(right)
            .cmp(&component_constraint_weight(left))
            .then_with(|| seeded_order(seed, &left.name).cmp(&seeded_order(seed, &right.name)))
    });

    let total = sum_component_counts(&components);
    let unavoidable_failures = individually_infeasible_floors(&components, &total);
    if !unavoidable_failures.is_empty() {
        return Err(render_failure(&unavoidable_failures));
    }

    let mut assignment = BTreeMap::new();
    let mut counts = [[0; FLOOR_COUNT]; 2];
    let mut best_failure = None;
    let mut failed_states = HashSet::new();
    if search_assignments(
        &components,
        0,
        &mut assignment,
        &mut counts,
        &total,
        &mut best_failure,
        &mut failed_states,
    ) {
        return Ok(SplitFeasibility {
            component_by_utterance_id,
            assignment_by_component: assignment,
        });
    }
    let best_failure = best_failure.expect("an infeasible search records a failed assignment");
    let failures: Vec<_> = FLOORS
        .iter()
        .enumerate()
        .filter_map(|(floor_index, floor)| {
            let observed =
                best_failure.counts[0][floor_index].min(best_failure.counts[1][floor_index]);
            (observed < floor.count).then_some((floor_index, observed))
        })
        .collect();
    Err(render_failure(&failures))
}

fn search_assignments(
    components: &[ComponentCounts],
    index: usize,
    assignment: &mut BTreeMap<String, Split>,
    counts: &mut [[usize; FLOOR_COUNT]; 2],
    remaining: &[usize; FLOOR_COUNT],
    best_failure: &mut Option<FailedAssignment>,
    failed_states: &mut HashSet<(usize, [usize; FLOOR_COUNT])>,
) -> bool {
    if floors_met(counts) {
        for component in &components[index..] {
            assignment.insert(component.name.clone(), Split::Validation);
        }
        return true;
    }

    if cannot_meet_floors(counts, remaining) {
        record_greedy_completion(components, index, assignment, counts, best_failure);
        return false;
    }
    if index == components.len() {
        update_best_failure(*counts, best_failure);
        return false;
    }
    if !failed_states.insert((index, counts[0])) {
        return false;
    }

    let component = &components[index];
    let mut next_remaining = *remaining;
    subtract_counts(&mut next_remaining, &component.counts);
    let branch_order = preferred_branch_order(counts, &component.counts);
    let branches: &[Split] = if index == 0 {
        &branch_order[..1]
    } else {
        &branch_order
    };
    for &split in branches {
        assignment.insert(component.name.clone(), split);
        add_counts(&mut counts[split_index(split)], &component.counts);
        if search_assignments(
            components,
            index + 1,
            assignment,
            counts,
            &next_remaining,
            best_failure,
            failed_states,
        ) {
            return true;
        }
        subtract_counts(&mut counts[split_index(split)], &component.counts);
    }
    assignment.remove(&component.name);
    false
}

fn floors_met(counts: &[[usize; FLOOR_COUNT]; 2]) -> bool {
    FLOORS
        .iter()
        .enumerate()
        .all(|(index, floor)| counts[0][index] >= floor.count && counts[1][index] >= floor.count)
}

fn counts_for_component(records: &[&GenerationOutput]) -> [usize; FLOOR_COUNT] {
    let mut counts = [0; FLOOR_COUNT];
    for record in records {
        let names = slice_names(record);
        for (index, floor) in FLOORS.iter().enumerate() {
            if floor.name != "paraphrase_clusters" && names.contains(floor.name) {
                counts[index] += 1;
            }
        }
    }

    let mut clusters: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for record in records {
        for tag in &record.intended_slice_tags {
            if let SliceTag::ParaphraseCluster { id } = tag {
                clusters.entry(id).or_default().insert(&record.utterance_id);
            }
        }
    }
    let cluster_index = FLOORS
        .iter()
        .position(|floor| floor.name == "paraphrase_clusters")
        .expect("paraphrase cluster floor exists");
    counts[cluster_index] = clusters
        .values()
        .filter(|utterances| utterances.len() >= 2)
        .count();
    counts
}

fn component_constraint_weight(component: &ComponentCounts) -> usize {
    component
        .counts
        .iter()
        .zip(FLOORS)
        .map(|(count, floor)| count.min(&floor.count))
        .sum()
}

fn sum_component_counts(components: &[ComponentCounts]) -> [usize; FLOOR_COUNT] {
    let mut total = [0; FLOOR_COUNT];
    for component in components {
        add_counts(&mut total, &component.counts);
    }
    total
}

fn individually_infeasible_floors(
    components: &[ComponentCounts],
    total: &[usize; FLOOR_COUNT],
) -> Vec<(usize, usize)> {
    FLOORS
        .iter()
        .enumerate()
        .filter_map(|(floor_index, floor)| {
            let mut reachable = vec![false; total[floor_index] + 1];
            reachable[0] = true;
            for component in components {
                let count = component.counts[floor_index];
                for observed in (count..reachable.len()).rev() {
                    reachable[observed] |= reachable[observed - count];
                }
            }
            let best = reachable
                .iter()
                .enumerate()
                .filter(|(_, reachable)| **reachable)
                .map(|(validation, _)| validation.min(total[floor_index] - validation))
                .max()
                .unwrap_or(0);
            (best < floor.count).then_some((floor_index, best))
        })
        .collect()
}

fn cannot_meet_floors(
    counts: &[[usize; FLOOR_COUNT]; 2],
    remaining: &[usize; FLOOR_COUNT],
) -> bool {
    FLOORS.iter().enumerate().any(|(index, floor)| {
        counts[0][index] + remaining[index] < floor.count
            || counts[1][index] + remaining[index] < floor.count
    })
}

fn record_greedy_completion(
    components: &[ComponentCounts],
    index: usize,
    assignment: &BTreeMap<String, Split>,
    counts: &[[usize; FLOOR_COUNT]; 2],
    best_failure: &mut Option<FailedAssignment>,
) {
    let mut completed_assignment = assignment.clone();
    let mut completed_counts = *counts;
    for component in &components[index..] {
        let split = preferred_branch_order(&completed_counts, &component.counts)[0];
        completed_assignment.insert(component.name.clone(), split);
        add_counts(&mut completed_counts[split_index(split)], &component.counts);
    }
    debug_assert_eq!(completed_assignment.len(), components.len());
    update_best_failure(completed_counts, best_failure);
}

fn preferred_branch_order(
    counts: &[[usize; FLOOR_COUNT]; 2],
    component: &[usize; FLOOR_COUNT],
) -> [Split; 2] {
    let improvement = |split: usize| {
        FLOORS
            .iter()
            .enumerate()
            .map(|(index, floor)| {
                component[index].min(floor.count.saturating_sub(counts[split][index]))
            })
            .sum::<usize>()
    };
    if improvement(1) > improvement(0) {
        [Split::Test, Split::Validation]
    } else {
        [Split::Validation, Split::Test]
    }
}

fn update_best_failure(
    counts: [[usize; FLOOR_COUNT]; 2],
    best_failure: &mut Option<FailedAssignment>,
) {
    let total_shortfall = FLOORS
        .iter()
        .enumerate()
        .map(|(index, floor)| {
            floor.count.saturating_sub(counts[0][index])
                + floor.count.saturating_sub(counts[1][index])
        })
        .sum();
    let minimum_ratio_millionths = FLOORS
        .iter()
        .enumerate()
        .map(|(index, floor)| counts[0][index].min(counts[1][index]) * 1_000_000 / floor.count)
        .min()
        .unwrap_or(0);
    let candidate = FailedAssignment {
        counts,
        total_shortfall,
        minimum_ratio_millionths,
    };
    let is_better = best_failure.as_ref().is_none_or(|best| {
        candidate.total_shortfall < best.total_shortfall
            || (candidate.total_shortfall == best.total_shortfall
                && candidate.minimum_ratio_millionths > best.minimum_ratio_millionths)
    });
    if is_better {
        *best_failure = Some(candidate);
    }
}

fn render_failure(failures: &[(usize, usize)]) -> String {
    let details = failures
        .iter()
        .map(|(index, observed)| {
            let floor = FLOORS[*index];
            format!(
                "{} requires {} utterances per split; best assignment reaches {observed}",
                floor.name, floor.count
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("split feasibility failed: {details}")
}

fn split_index(split: Split) -> usize {
    match split {
        Split::Validation => 0,
        Split::Test => 1,
    }
}

fn add_counts(target: &mut [usize; FLOOR_COUNT], added: &[usize; FLOOR_COUNT]) {
    for (target, added) in target.iter_mut().zip(added) {
        *target += added;
    }
}

fn subtract_counts(target: &mut [usize; FLOOR_COUNT], removed: &[usize; FLOOR_COUNT]) {
    for (target, removed) in target.iter_mut().zip(removed) {
        *target -= removed;
    }
}

fn slice_names(record: &GenerationOutput) -> BTreeSet<&'static str> {
    let mut names = BTreeSet::new();
    if record.conditioning_goal_ref.is_none() {
        names.insert("none_of_roster");
    }
    for tag in &record.intended_slice_tags {
        match tag {
            SliceTag::ExplicitNegation => {
                names.insert("negation");
                names.insert("explicit_negation");
            }
            SliceTag::ImplicitNegation => {
                names.insert("negation");
                names.insert("implicit_negation");
            }
            SliceTag::QuotedSpeech => {
                names.insert("quoted_speech");
            }
            SliceTag::Hypothetical => {
                names.insert("hypothetical");
            }
            SliceTag::SubjectConfusion => {
                names.insert("subject_confusion");
            }
            SliceTag::PunctuationCasingLoss => {
                names.insert("punctuation_casing_loss");
            }
            SliceTag::SyntheticAsr => {
                names.insert("synthetic_asr");
            }
            SliceTag::RareHighCost => {
                names.insert("rare_high_cost");
            }
            SliceTag::HardNegative => {
                names.insert("hard_negative");
            }
            SliceTag::ParaphraseCluster { .. } | SliceTag::RealAsr => {}
        }
    }
    names
}

pub fn connected_components(
    records: &[GenerationOutput],
) -> Result<BTreeMap<String, String>, String> {
    let mut parent = Vec::with_capacity(records.len());
    let mut sessions = HashMap::new();
    let mut clusters = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        if record.session_id.is_empty() || record.semantic_cluster_id.is_empty() {
            return Err(
                "generation output has an empty session_id or semantic_cluster_id".to_string(),
            );
        }
        parent.push(index);
        if let Some(previous) = sessions.insert(record.session_id.as_str(), index) {
            union(&mut parent, index, previous);
        }
        if let Some(previous) = clusters.insert(record.semantic_cluster_id.as_str(), index) {
            union(&mut parent, index, previous);
        }
    }
    let mut canonical = BTreeMap::new();
    for (index, _record) in records.iter().enumerate() {
        let root = find(&mut parent, index);
        canonical
            .entry(root)
            .or_insert_with(|| format!("component-{root:04}"));
    }
    Ok(records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let root = find(&mut parent, index);
            (
                record.utterance_id.clone(),
                canonical.get(&root).expect("component").clone(),
            )
        })
        .collect())
}

fn find(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        let root = find(parent, parent[index]);
        parent[index] = root;
    }
    parent[index]
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left = find(parent, left);
    let right = find(parent, right);
    if left != right {
        parent[right] = left;
    }
}

fn seeded_order(seed: u64, value: &str) -> u64 {
    value
        .bytes()
        .fold(seed ^ 0x9e37_79b9_7f4a_7c15, |hash, byte| {
            hash.rotate_left(5) ^ u64::from(byte)
        })
}
