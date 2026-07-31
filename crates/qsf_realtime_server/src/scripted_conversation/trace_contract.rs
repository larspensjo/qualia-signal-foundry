use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceContractReport {
    pub complete: bool,
    pub turns: Vec<TraceTurnReport>,
    pub parse_errors: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceTurnReport {
    pub exchange_index: usize,
    pub trusted_exchange: bool,
    pub volition_context_injected: bool,
    pub turn_context_captured: bool,
    pub matching_request_hash: bool,
}

pub fn parse_trace_contract(bytes: &[u8], promoted_indices: &[usize]) -> TraceContractReport {
    let mut records: HashMap<usize, TraceTurnReport> = promoted_indices
        .iter()
        .map(|index| {
            (
                *index,
                TraceTurnReport {
                    exchange_index: *index,
                    ..TraceTurnReport::default()
                },
            )
        })
        .collect();
    let mut capture_hashes: HashMap<usize, String> = HashMap::new();
    let mut injection_hashes: HashMap<usize, String> = HashMap::new();
    let mut parse_errors = Vec::new();
    for (line_number, line) in String::from_utf8_lossy(bytes).lines().enumerate() {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                parse_errors.push(format!("line {}: {error}", line_number + 1));
                continue;
            }
        };
        let kind = value.get("kind").and_then(|item| item.as_str());
        let index = value
            .get("exchange_index")
            .and_then(|item| item.as_u64())
            .map(|item| item as usize);
        match (kind, index) {
            (Some("volition_context_injected"), Some(index)) if records.contains_key(&index) => {
                records
                    .get_mut(&index)
                    .expect("known")
                    .volition_context_injected = true;
                if let Some(hash) = value
                    .pointer("/trace/request_hash")
                    .and_then(|item| item.as_str())
                {
                    injection_hashes.insert(index, hash.to_string());
                }
            }
            (Some("turn_context_captured"), Some(index)) if records.contains_key(&index) => {
                records
                    .get_mut(&index)
                    .expect("known")
                    .turn_context_captured = true;
                if let Some(hash) = value.get("request_hash").and_then(|item| item.as_str()) {
                    capture_hashes.insert(index, hash.to_string());
                }
            }
            (Some("diagnostic_exchange_recorded"), _)
                if value.get("source").and_then(|item| item.as_str())
                    == Some("sideband_trusted") =>
            {
                let exchange = value.get("exchange");
                let index = exchange
                    .and_then(|item| item.get("index"))
                    .and_then(|item| item.as_u64())
                    .map(|item| item as usize);
                if let Some(index) = index.filter(|index| records.contains_key(index)) {
                    records.get_mut(&index).expect("known").trusted_exchange = true;
                }
            }
            _ => {}
        }
    }
    for (index, report) in &mut records {
        report.matching_request_hash = capture_hashes
            .get(index)
            .zip(injection_hashes.get(index))
            .is_some_and(|(capture, injection)| capture == injection);
    }
    let mut turns: Vec<_> = records.into_values().collect();
    turns.sort_by_key(|turn| turn.exchange_index);
    let complete = parse_errors.is_empty()
        && turns.iter().all(|turn| {
            turn.trusted_exchange
                && turn.volition_context_injected
                && turn.turn_context_captured
                && turn.matching_request_hash
        });
    TraceContractReport {
        complete,
        turns,
        parse_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_trace_contract;

    const COMPLETE_LEDGER: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/Experiments/Fixtures/realtime-probe/trace-contract.complete.jsonl"
    ));

    #[test]
    fn complete_turn_passes() {
        let report = parse_trace_contract(COMPLETE_LEDGER, &[2]);

        assert!(report.complete);
        assert!(report.parse_errors.is_empty());
        assert!(report.turns[0].matching_request_hash);
    }

    #[test]
    fn missing_injection_is_reported_for_its_turn() {
        let input = String::from_utf8_lossy(COMPLETE_LEDGER)
            .lines()
            .filter(|line| !line.contains("volition_context_injected"))
            .collect::<Vec<_>>()
            .join("\n");
        let report = parse_trace_contract(input.as_bytes(), &[2]);

        assert!(!report.complete);
        assert!(report.parse_errors.is_empty());
        assert_eq!(report.turns[0].exchange_index, 2);
        assert!(!report.turns[0].volition_context_injected);
    }

    #[test]
    fn mismatched_request_hash_is_reported_for_its_turn() {
        let input =
            String::from_utf8_lossy(COMPLETE_LEDGER).replacen("sha256:turn-2", "sha256:other", 1);
        let report = parse_trace_contract(input.as_bytes(), &[2]);

        assert!(!report.complete);
        assert!(report.parse_errors.is_empty());
        assert_eq!(report.turns[0].exchange_index, 2);
        assert!(!report.turns[0].matching_request_hash);
    }

    #[test]
    fn injection_hash_from_another_turn_cannot_satisfy_linkage() {
        let input = String::from_utf8_lossy(COMPLETE_LEDGER).replace(
            "\"kind\":\"volition_context_injected\",\"qsf_session_id\":\"default\",\"exchange_index\":2",
            "\"kind\":\"volition_context_injected\",\"qsf_session_id\":\"default\",\"exchange_index\":3",
        );
        let report = parse_trace_contract(input.as_bytes(), &[2, 3]);

        assert!(!report.complete);
        assert!(!report.turns[0].matching_request_hash);
    }
}
