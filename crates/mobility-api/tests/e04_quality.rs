use std::{fs, path::Path, process::Command};

#[test]
fn e04_s04_given_missing_and_invalid_data_when_rendered_then_dashboard_is_accessible_and_escaped() {
    use mobility_ingest::connectors::normalization::{build, load, Recipe};
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/normalization");
    // Avoid introducing an external dependency into the API crate.
    let temp = std::env::temp_dir().join(format!("carfree-e04-dashboard-{}", std::process::id()));
    fs::create_dir(&temp).unwrap();
    // Use the public CLI against an immutable fixture built by the ingest CLI in its own tests;
    // here, build a tiny recipe using the public v1 types and no direct JSON decoder dependency.
    let recipe = Recipe {
        schema_version: 1,
        as_of: "2026-03-09".into(),
        expected: [("places".into(), ["<script>alert(1)</script>".into()].into())].into(),
        inputs: vec![],
        stop_fixes: vec![],
        walk_entries: Default::default(),
    };
    let id = build(recipe, &fixtures, &temp).unwrap();
    let snapshot = load(&temp, &id).unwrap();
    assert_eq!(snapshot.quality["places"].confidence, "unavailable");
    let output = Command::new(env!("CARGO_BIN_EXE_quality-dashboard"))
        .arg(&temp)
        .arg(&id)
        .output()
        .unwrap();
    fs::remove_dir_all(&temp).unwrap();
    assert!(output.status.success());
    let html = String::from_utf8(output.stdout).unwrap();
    for text in [
        "<html lang=\"en\">",
        "<caption>Quality by input layer</caption>",
        "scope=\"col\"",
        "scope=\"row\"",
        "Missing regions",
        "Invalid findings",
        "Confidence tier",
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        &id,
    ] {
        assert!(html.contains(text), "missing {text}");
    }
    assert!(!html.contains("<script>"));
}
