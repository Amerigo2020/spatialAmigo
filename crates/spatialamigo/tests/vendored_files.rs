//! The crate ships copies of `schema/*.json` and `examples/scenes/tabletop.json` so that
//! `cargo package` works. The repository-root files stay the single source of truth; this
//! test fails as soon as a copy drifts. Outside the repository (packaged crate) it is a no-op.

use std::path::Path;

#[test]
fn vendored_copies_match_repository_originals() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.join("../..");
    if !root.join("schema").is_dir() {
        return;
    }
    let pairs = [
        ("schema/scene-v0.1.json", "schema/scene-v0.1.json"),
        ("schema/query-v0.1.json", "schema/query-v0.1.json"),
        ("tests/fixtures/tabletop.json", "examples/scenes/tabletop.json"),
    ];
    for (copy, original) in pairs {
        let a = std::fs::read(crate_dir.join(copy)).unwrap();
        let b = std::fs::read(root.join(original)).unwrap();
        assert!(
            a == b,
            "{copy} differs from {original}; copy the original into the crate"
        );
    }
}
