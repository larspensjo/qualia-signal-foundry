use std::cmp::Ordering;
use std::time::SystemTime;

use super::{
    Exchange, InterruptionRecord, ProviderEventKind, ProviderEventRecord, RecallRecord,
    SessionState, Turn,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SleepRecord<'a> {
    Turn(&'a Turn),
    Exchange(&'a Exchange),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SleepRecordKind {
    Turn,
    Exchange,
}

impl SessionState {
    pub fn sleep_records(&self) -> Vec<SleepRecord<'_>> {
        let mut records = Vec::with_capacity(self.turns.len() + self.exchanges.len());
        records.extend(self.turns.iter().map(SleepRecord::Turn));
        records.extend(self.exchanges.iter().map(SleepRecord::Exchange));
        records.sort_by(compare_sleep_records);
        records
    }
}

impl<'a> SleepRecord<'a> {
    pub fn kind(&self) -> SleepRecordKind {
        match self {
            Self::Turn(_) => SleepRecordKind::Turn,
            Self::Exchange(_) => SleepRecordKind::Exchange,
        }
    }

    pub fn source_index(&self) -> usize {
        match self {
            Self::Turn(turn) => turn.index,
            Self::Exchange(exchange) => exchange.index,
        }
    }

    pub fn started_at(&self) -> SystemTime {
        match self {
            Self::Turn(turn) => turn.started_at,
            Self::Exchange(exchange) => exchange.started_at,
        }
    }

    pub fn completed_at(&self) -> SystemTime {
        match self {
            Self::Turn(turn) => turn.completed_at,
            Self::Exchange(exchange) => exchange.completed_at.unwrap_or(exchange.started_at),
        }
    }

    pub fn user_input_text(&self) -> &str {
        match self {
            Self::Turn(turn) => &turn.user_input,
            Self::Exchange(exchange) => exchange.final_user_input(),
        }
    }

    pub fn assistant_output_text(&self) -> Option<&str> {
        match self {
            Self::Turn(turn) => Some(turn.assistant_response.as_str()),
            Self::Exchange(exchange) => exchange.output.as_ref().map(|output| output.text.as_str()),
        }
    }

    pub fn retrieved_memory_block(&self) -> &str {
        match self {
            Self::Turn(turn) => &turn.retrieved_memory_block,
            Self::Exchange(exchange) => &exchange.retrieved_memory_block,
        }
    }

    pub fn recalled_items(&self) -> &[RecallRecord] {
        match self {
            Self::Turn(turn) => &turn.recalled_turns,
            Self::Exchange(exchange) => &exchange.recalled_items,
        }
    }

    pub fn interruption_records(&self) -> &[InterruptionRecord] {
        match self {
            Self::Turn(_) => &[],
            Self::Exchange(exchange) => &exchange.interruptions,
        }
    }

    pub fn provider_events(&self) -> &[ProviderEventRecord] {
        match self {
            Self::Turn(_) => &[],
            Self::Exchange(exchange) => &exchange.provider_events,
        }
    }

    pub fn retrieval_source_ids(&self) -> Vec<String> {
        match self {
            Self::Turn(turn) => turn.context_assembly.retrieved_memory_ids().to_vec(),
            Self::Exchange(exchange) => exchange
                .context_assembly
                .as_ref()
                .map(|context| context.retrieved_memory_ids().to_vec())
                .unwrap_or_default(),
        }
    }

    pub fn final_transcript(&self) -> Option<&str> {
        match self {
            Self::Turn(_) => None,
            Self::Exchange(exchange) => Some(exchange.final_user_input()),
        }
    }

    pub fn provider_diagnostic_notes(&self) -> Vec<String> {
        if !matches!(self, Self::Exchange(_)) {
            return vec![];
        }

        let mut notes = Vec::new();
        if !self.provider_events().is_empty() {
            notes.push(summarize_provider_events(self.provider_events()));
        }
        if !self.interruption_records().is_empty() {
            notes.push(summarize_interruptions(self.interruption_records()));
        }

        notes
    }
}

fn compare_sleep_records(left: &SleepRecord<'_>, right: &SleepRecord<'_>) -> Ordering {
    compare_system_time(left.started_at(), right.started_at())
        .then_with(|| compare_system_time(left.completed_at(), right.completed_at()))
        .then_with(|| left.kind().cmp(&right.kind()))
        .then_with(|| left.source_index().cmp(&right.source_index()))
}

fn compare_system_time(left: SystemTime, right: SystemTime) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

fn summarize_provider_events(events: &[ProviderEventRecord]) -> String {
    let mut preambles = 0usize;
    let mut response_started = 0usize;
    let mut response_completed = 0usize;
    let mut speech_playback_started = 0usize;
    let mut speech_playback_completed = 0usize;
    let mut provider_ids = Vec::new();

    for event in events {
        if !provider_ids
            .iter()
            .any(|provider_id| provider_id == &event.provider_id)
        {
            provider_ids.push(event.provider_id.clone());
        }

        match event.event_kind {
            ProviderEventKind::Preamble => preambles += 1,
            ProviderEventKind::ResponseStarted => response_started += 1,
            ProviderEventKind::ResponseCompleted => response_completed += 1,
            ProviderEventKind::SpeechPlaybackStarted => speech_playback_started += 1,
            ProviderEventKind::SpeechPlaybackCompleted => speech_playback_completed += 1,
        }
    }

    format!(
        "Provider diagnostics: providers={} preambles={} response_started={} response_completed={} speech_playback_started={} speech_playback_completed={}",
        provider_ids.join(","),
        preambles,
        response_started,
        response_completed,
        speech_playback_started,
        speech_playback_completed
    )
}

fn summarize_interruptions(interruptions: &[InterruptionRecord]) -> String {
    let details = interruptions
        .iter()
        .map(|interruption| {
            let response_id = interruption
                .response_id
                .as_deref()
                .map(|value| format!(" response_id={value}"))
                .unwrap_or_default();
            let partial_response_present = interruption
                .partial_response_text
                .as_ref()
                .map(|text| !text.trim().is_empty())
                .unwrap_or(false);
            format!(
                "source={} action={:?} stop_outcome={:?} partial_response_present={partial_response_present}{response_id}",
                interruption.source, interruption.action, interruption.stop_outcome
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "Interruption diagnostics: count={} details={details}",
        interruptions.len()
    )
}
