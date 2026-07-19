use std::{env, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("qsf_semantic_eval: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("write-roster-snapshot") => {
            let path = args
                .next()
                .ok_or_else(|| "write-roster-snapshot requires a destination path".to_string())?;
            if args.next().is_some() {
                return Err(
                    "write-roster-snapshot accepts exactly one destination path".to_string()
                );
            }
            qsf_semantic_eval::RosterSnapshot::from_realtime_seed().write_json_path(path)
        }
        output => {
            if args.next().is_some() {
                return Err("usage: qsf_semantic_eval [output-directory]".to_string());
            }
            let default_output = qsf_semantic_eval::default_run_output_dir();
            let output = output
                .map(std::path::PathBuf::from)
                .unwrap_or(default_output);
            let (dataset, snapshot) = qsf_semantic_eval::load_default_inputs()?;
            let results = qsf_semantic_eval::run_baseline(&dataset, &snapshot)?;
            let report = qsf_semantic_eval::build_report_for_dataset(&results, &dataset)?;
            let artifacts = qsf_semantic_eval::write_report_artifacts(output, &results, &report)?;
            println!("wrote {}", artifacts.metrics_json.display());
            Ok(())
        }
    }
}
