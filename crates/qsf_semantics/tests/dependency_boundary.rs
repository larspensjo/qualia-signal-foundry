use std::fs;

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
