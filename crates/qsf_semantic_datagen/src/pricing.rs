use serde::Deserialize;

use crate::sha256;

pub const PRICE_TABLE: &str = include_str!("../pricing/goalrel-generation-price-table.v1.json");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub cached_input_tokens: u32,
    pub output_tokens: u32,
}

impl TokenUsage {
    pub(crate) fn add(&mut self, other: Self) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceTable {
    pub version: String,
    pub provenance_date: String,
    pub source_url: String,
    pub content_sha256: String,
    pub entries: Vec<PriceEntry>,
}

#[derive(serde::Serialize)]
struct PriceTableMaterial<'a> {
    version: &'a str,
    provenance_date: &'a str,
    source_url: &'a str,
    entries: &'a [PriceEntry],
}

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct PriceEntry {
    pub model_id: String,
    pub input_usd_per_million: f64,
    pub cached_input_usd_per_million: f64,
    pub output_usd_per_million: f64,
}

pub fn tool_local_price_table() -> Result<PriceTable, String> {
    let table: PriceTable = serde_json::from_str(PRICE_TABLE)
        .map_err(|error| format!("invalid tool-local price table: {error}"))?;
    let material = PriceTableMaterial {
        version: &table.version,
        provenance_date: &table.provenance_date,
        source_url: &table.source_url,
        entries: &table.entries,
    };
    let computed =
        sha256(&serde_json::to_string(&material).expect("price table material serializes"));
    if table.content_sha256 != computed {
        return Err(format!(
            "price table content hash mismatch: stored {}, computed {computed}",
            table.content_sha256
        ));
    }
    Ok(table)
}

pub fn estimate_cost_usd(table: &PriceTable, model_id: &str, usage: TokenUsage) -> Option<f64> {
    let entry = table
        .entries
        .iter()
        .find(|entry| entry.model_id == model_id)?;
    let cached = usage.cached_input_tokens.min(usage.input_tokens);
    let fresh = usage.input_tokens - cached;
    Some(
        f64::from(fresh) * entry.input_usd_per_million / 1_000_000.0
            + f64::from(cached) * entry.cached_input_usd_per_million / 1_000_000.0
            + f64::from(usage.output_tokens) * entry.output_usd_per_million / 1_000_000.0,
    )
}

pub fn render_usage_report(model_id: &str, usage: TokenUsage) -> String {
    let tokens = format!(
        "generation token usage model_id={model_id}: input={} (cached={}), output={}",
        usage.input_tokens, usage.cached_input_tokens, usage.output_tokens
    );
    match tool_local_price_table()
        .ok()
        .and_then(|table| estimate_cost_usd(&table, model_id, usage))
    {
        Some(cost) => format!("{tokens}; estimated cost=${cost:.6} USD (tool-local price table)"),
        None => {
            format!("{tokens}; estimated cost unavailable (no matching tool-local price entry)")
        }
    }
}
