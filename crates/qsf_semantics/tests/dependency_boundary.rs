use std::fs;
use std::process::Command;

#[test]
fn manifest_excludes_domain_crates() {
    // `cargo metadata --offline` is not usable in the repository's restricted test
    // environment because resolving the whole workspace tries to unpack uncached
    // target-specific registry packages. This deliberately checks direct and dev
    // dependencies only; it can false-positive on a comment and cannot see a
    // transitive path.
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("crate manifest");
    for forbidden in [
        "qsf_app",
        "qsf_realtime_server",
        "qsf_volition",
        "qsf_memory",
        "qsf_semantic_eval",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "forbidden dependency {forbidden}"
        );
    }
}

#[test]
fn memory_normal_dependencies_exclude_http_runtime() {
    let output = Command::new("cargo")
        .args(["tree", "-p", "qsf_memory", "-e", "normal", "-f", "{p}"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).expect("UTF-8 dependency tree");
    for forbidden in ["reqwest v", "tokio v", "futures-util v"] {
        assert!(
            !tree.contains(forbidden),
            "memory normal dependency tree contains {forbidden}"
        );
    }
}
