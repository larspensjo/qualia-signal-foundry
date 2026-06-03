use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::text_owned_voice_loop::TextOwnedVoiceLoopExperiment;

pub(crate) const VOICE_LOOP_DESCRIPTION: &str = "Run the peer voice-loop surface through the same QSF-owned voice pipeline as text-owned voice loop";

pub struct VoiceLoopExperiment;

impl Experiment for VoiceLoopExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VoiceLoop
    }

    fn description(&self) -> &'static str {
        VOICE_LOOP_DESCRIPTION
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        TextOwnedVoiceLoopExperiment.run(context)
    }
}
