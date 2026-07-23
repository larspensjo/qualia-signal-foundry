use std::process::ExitCode;

fn main() -> ExitCode {
    engine_logging::initialize();
    match qsf_semantic_datagen::run_cli(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("qsf_semantic_datagen: {error}");
            ExitCode::FAILURE
        }
    }
}
