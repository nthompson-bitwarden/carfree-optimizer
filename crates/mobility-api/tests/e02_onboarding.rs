//! E02-S02..S06 CLI acceptance tests. Synthetic drawn inputs only exercise the
//! onboarding contract; they are not observed transport data or model results.
use mobility_ingest::study_area::CityProject;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDirectory(PathBuf);
impl TempDirectory {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "carfree-e02-onboarding-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_study-area"))
        .args(args)
        .output()
        .unwrap()
}
fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn succeeds(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn load(path: &Path) -> CityProject {
    CityProject::from_tsv(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn e02_s02_s03_s04_given_canonical_city_when_initialized_and_previewed_then_boundary_and_data_gaps_are_visible(
) {
    let temp = TempDirectory::new();
    let project_path = temp.file("project.tsv");
    let html_path = temp.file("preview.html");
    succeeds(cli(&[
        "init",
        "place:4159000",
        "2026-11-17",
        path(&project_path),
    ]));
    let project = load(&project_path);
    assert_eq!(project.area().id(), "place:4159000");
    assert_eq!(project.snapshot_date(), "2026-11-17");
    assert_eq!(project.area().crs(), "EPSG:4326");
    assert_eq!(project.area().rings().len(), 9);
    assert!(!project.baseline_id().is_empty());
    assert_eq!(project.coverage().missing_required.len(), 4);
    succeeds(cli(&["preview", path(&project_path), path(&html_path)]));
    let html = std::fs::read_to_string(&html_path).unwrap();
    let lower = html.to_ascii_lowercase();
    for text in [
        "portland",
        "epsg:4326",
        "2026-11-17",
        "2024",
        "census.gov",
        "roads",
        "transit",
        "population",
        "demand",
        "missing",
    ] {
        assert!(
            lower.contains(text),
            "missing human-readable metadata {text}"
        );
    }
    assert!(lower.contains("<svg"));
    assert!(lower.contains("role=\"img\""));
    assert!(lower.contains("<title"));
    assert!(lower.contains("<desc"));
    assert!(lower.contains("<th"));
    assert!(lower.contains("longitude") && lower.contains("latitude"));
    assert!(!lower.contains("<script"));
    let before = std::fs::read(&project_path).unwrap();
    let overwrite = cli(&["init", "place:2360545", "2026-11-17", path(&project_path)]);
    assert!(!overwrite.status.success());
    assert!(!overwrite.stderr.is_empty());
    assert_eq!(std::fs::read(&project_path).unwrap(), before);
    assert!(!cli(&["preview", path(&project_path), path(&html_path)])
        .status
        .success());
}

#[test]
fn e02_s02_given_drawn_polygon_when_saved_then_metadata_and_geometry_roundtrip_and_html_is_escaped()
{
    let temp = TempDirectory::new();
    let rings_path = temp.file("drawn-rings.tsv");
    let project_path = temp.file("drawn.tsv");
    let html_path = temp.file("drawn.html");
    std::fs::write(
        &rings_path,
        "ring\t-123,45\t-122.9,45\t-122.9,45.1\t-123,45.1\t-123,45\n",
    )
    .unwrap();
    succeeds(cli(&[
        "draw",
        "synthetic-area",
        "<script>alert(1)</script>",
        "OR",
        "2024-02-29",
        path(&rings_path),
        path(&project_path),
    ]));
    let project = load(&project_path);
    assert_eq!(project.area().name(), "<script>alert(1)</script>");
    assert_eq!(project.area().region().states, ["OR"]);
    assert_eq!(project.area().rings()[0].len(), 5);
    assert_eq!(project.area().rings()[0][0].longitude, -123.0);
    assert_eq!(project.area().rings()[0][2].latitude, 45.1);
    succeeds(cli(&["preview", path(&project_path), path(&html_path)]));
    let html = std::fs::read_to_string(&html_path).unwrap();
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    let before = std::fs::read(&project_path).unwrap();
    succeeds(cli(&["scenario", path(&project_path), "before-buffer"]));
    assert_ne!(std::fs::read(&project_path).unwrap(), before);
    succeeds(cli(&["buffer", path(&project_path), "5"]));
    let changed = load(&project_path);
    assert_eq!(changed.import_attempts(), 2);
    assert_eq!(changed.scenarios()[0].id, "before-buffer");
    assert!(changed.scenarios()[0].stale);
    assert_eq!(changed.area().core_rings()[0].len(), 5);
}

#[test]
fn e02_s05_given_saved_scenario_when_buffer_changes_then_preview_shows_stale_baseline_and_import_rerun(
) {
    let temp = TempDirectory::new();
    let project_path = temp.file("metro.tsv");
    let html_path = temp.file("expanded.html");
    succeeds(cli(&[
        "init",
        "cbsa:38900",
        "2026-11-17",
        path(&project_path),
    ]));
    succeeds(cli(&["scenario", path(&project_path), "prior-plan"]));
    let before = load(&project_path);
    succeeds(cli(&["buffer", path(&project_path), "10"]));
    let after = load(&project_path);
    assert_ne!(after.baseline_id(), before.baseline_id());
    assert_eq!(after.revision(), before.revision() + 1);
    assert_eq!(after.import_attempts(), 2);
    assert!(after.scenarios()[0].stale);
    assert_eq!(after.scenarios()[0].baseline_id, before.baseline_id());
    succeeds(cli(&["preview", path(&project_path), path(&html_path)]));
    let html = std::fs::read_to_string(html_path)
        .unwrap()
        .to_ascii_lowercase();
    assert!(html.contains("prior-plan") && html.contains("stale"));
    assert!(
        html.contains("envelope"),
        "buffer geometry approximation must be disclosed"
    );
    assert!(html.contains("partial") || html.contains("missing"));
    let unchanged = std::fs::read(&project_path).unwrap();
    succeeds(cli(&["buffer", path(&project_path), "10"]));
    assert_eq!(std::fs::read(&project_path).unwrap(), unchanged);
    assert!(!cli(&["buffer", path(&project_path), "NaN"])
        .status
        .success());
    assert_eq!(std::fs::read(&project_path).unwrap(), unchanged);
}

#[test]
fn e02_s06_given_mixed_catalogue_requests_when_batch_runs_then_per_item_results_and_idempotent_files_survive_restarts(
) {
    let temp = TempDirectory::new();
    let requests_path = temp.file("requests.tsv");
    let output_dir = temp.file("projects");
    std::fs::write(&requests_path, "portland-me\tplace:2360545\t2026-11-17\nunsupported\tplace:9999999\t2026-11-17\nportland-metro\tcbsa:38900\t2026-11-17\n").unwrap();
    let first = cli(&["batch", path(&requests_path), path(&output_dir)]);
    assert!(
        !first.status.success(),
        "mixed batch must expose a failure exit status"
    );
    let report = String::from_utf8(first.stdout)
        .unwrap()
        .to_ascii_lowercase();
    for id in ["portland-me", "unsupported", "portland-metro"] {
        assert!(report.contains(id), "missing per-item status for {id}");
    }
    assert!(report.contains("created"));
    assert!(report.contains("failed"));
    let me = output_dir.join("portland-me.tsv");
    let metro = output_dir.join("portland-metro.tsv");
    assert_eq!(load(&me).area().id(), "place:2360545");
    assert_eq!(load(&metro).area().id(), "cbsa:38900");
    assert!(!output_dir.join("unsupported.tsv").exists());
    let before_me = std::fs::read(&me).unwrap();
    let before_metro = std::fs::read(&metro).unwrap();
    let again = cli(&["batch", path(&requests_path), path(&output_dir)]);
    assert!(!again.status.success());
    let repeated = String::from_utf8(again.stdout)
        .unwrap()
        .to_ascii_lowercase();
    assert_eq!(repeated.matches("unchanged").count(), 2);
    assert_eq!(std::fs::read(&me).unwrap(), before_me);
    assert_eq!(std::fs::read(&metro).unwrap(), before_metro);
    assert_eq!(std::fs::read_dir(&output_dir).unwrap().count(), 2);
}

#[test]
fn e02_s06_given_conflicting_or_unsafe_project_ids_then_batch_preserves_existing_projects_and_processes_other_rows(
) {
    let temp = TempDirectory::new();
    let requests_path = temp.file("requests.tsv");
    let output_dir = temp.file("projects");
    std::fs::write(&requests_path, "same-id\tplace:2360545\t2026-11-17\nsame-id\tplace:4159000\t2026-11-17\n../escape\tcbsa:38900\t2026-11-17\ngood-after-bad\tcbsa:38900\t2026-11-17\n").unwrap();
    let output = cli(&["batch", path(&requests_path), path(&output_dir)]);
    assert!(!output.status.success());
    let report = String::from_utf8(output.stdout)
        .unwrap()
        .to_ascii_lowercase();
    assert_eq!(report.matches("failed").count(), 2);
    assert_eq!(
        load(&output_dir.join("same-id.tsv")).area().id(),
        "place:2360545"
    );
    assert_eq!(
        load(&output_dir.join("good-after-bad.tsv")).area().id(),
        "cbsa:38900"
    );
    assert!(!temp.file("escape.tsv").exists());
    assert_eq!(std::fs::read_dir(&output_dir).unwrap().count(), 2);
}

#[test]
fn e02_s02_s04_given_invalid_cli_inputs_then_errors_are_explicit_and_no_project_is_written() {
    let temp = TempDirectory::new();
    let project_path = temp.file("invalid.tsv");
    let ring_path = temp.file("bad-rings.tsv");
    std::fs::write(
        &ring_path,
        "ring\t-123,45\t-122,46\t-123,46\t-122,45\t-123,45\n",
    )
    .unwrap();
    for args in [
        vec![],
        vec!["init", "place:9999999", "2026-11-17", path(&project_path)],
        vec!["init", "place:2360545", "2023-02-29", path(&project_path)],
        vec![
            "draw",
            "synthetic",
            "Invalid polygon",
            "OR",
            "2026-11-17",
            path(&ring_path),
            path(&project_path),
        ],
        vec!["preview", path(&project_path), path(&temp.0)],
        vec!["buffer", path(&project_path), "-1"],
        vec!["unknown"],
    ] {
        let output = cli(&args);
        assert!(!output.status.success(), "unexpected CLI success {args:?}");
        assert!(!output.stderr.is_empty(), "missing error {args:?}");
        assert!(!project_path.exists());
    }
}
