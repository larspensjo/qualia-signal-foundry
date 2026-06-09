use std::path::PathBuf;

pub use qsf_session::resume::{
    LoadResumeInputsResult, ResumeInputs, SchemaUpgrade, classify_resume_mode,
    load_resume_inputs_with_upgrade,
};

pub fn load_resume_inputs(state_dir: impl AsRef<std::path::Path>) -> anyhow::Result<ResumeInputs> {
    let result: LoadResumeInputsResult = load_resume_inputs_with_upgrade(state_dir)?;
    if let Some(upgrade) = &result.schema_upgrade {
        engine_logging::engine_info!(
            "session state schema upgraded on resume: session_id={} from={} to={}",
            upgrade.session_id,
            upgrade.from,
            upgrade.to
        );
    }
    Ok(result.inputs)
}

pub fn state_dir_from_env() -> PathBuf {
    crate::session::resolve_shared_state_directory_from_env().resume_state_dir
}
