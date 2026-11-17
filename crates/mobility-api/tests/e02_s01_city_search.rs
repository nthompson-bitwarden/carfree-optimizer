//! E02-S01 observable HTML and CLI acceptance tests, with synthetic injection
//! records explicitly distinct from the real Census pilot geography.
use mobility_api::geography::render_search_results;
use mobility_ingest::geography::Catalogue;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_city-search"))
}

struct TempDirectory(PathBuf);
impl TempDirectory {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "carfree-e02-s01-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn e02_s01_given_ambiguous_city_query_then_html_displays_all_named_actual_boundaries() {
    let html = render_search_results(&Catalogue::pilot().unwrap(), "Portland").unwrap();
    assert!(html.to_ascii_lowercase().contains("<!doctype html>"));
    for text in [
        "place:2360545",
        "place:4159000",
        "cbsa:38900",
        "Portland city",
        "Portland-Vancouver-Hillsboro",
        "ME",
        "OR",
        "WA",
        "2024",
        "census.gov",
    ] {
        assert!(
            html.contains(text),
            "missing readable canonical identity/provenance {text}"
        );
    }
    assert_eq!(
        html.matches("<svg").count(),
        3,
        "each candidate needs its own boundary display"
    );
    assert!(html.matches("<path").count() >= 3);
    assert!(
        html.contains(" d=\"M"),
        "nonempty coordinate-derived SVG path"
    );
    assert!(html.contains("evenodd"), "preserve boundary holes");
    assert_eq!(html.matches("role=\"img\"").count(), 3);
    assert!(html.matches("<title").count() >= 3);
    assert!(html.matches("<desc").count() >= 3);
    let lower = html.to_ascii_lowercase();
    assert!(lower.contains("coverage") && lower.contains("pilot"));
    assert!(lower.contains("longitude") && lower.contains("latitude"));
}

#[test]
fn e02_s01_given_unknown_city_then_html_explicitly_explains_coverage_without_a_boundary() {
    let html =
        render_search_results(&Catalogue::pilot().unwrap(), "Unrepresented city 12345").unwrap();
    let lower = html.to_ascii_lowercase();
    assert!(lower.contains("no ") && (lower.contains("match") || lower.contains("result")));
    assert!(lower.contains("coverage"));
    assert!(!html.contains("<svg"));
    assert!(!html.contains("<path"));
}

#[test]
fn e02_s01_given_untrusted_catalogue_metadata_then_html_escapes_active_markup() {
    // Synthetic test polygon and metadata, not a real geography.
    let input = "carfree-geography-v1\narea\tplace:0000001\tcity\t<script>alert(1)</script>\tOR\thttps://example.org/?a=1&b=2\t2024\t2026-11-17\t<img src=x onerror=alert(2)>\nring\t-123,45\t-122,45\t-122,46\t-123,45\n";
    let catalogue = Catalogue::from_tsv(input).unwrap();
    let html = render_search_results(&catalogue, "place:0000001").unwrap();
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<img src=x"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&lt;img"));
    assert!(html.contains("a=1&amp;b=2"));
    assert!(render_search_results(&catalogue, "\n").is_err());
}

#[test]
fn e02_s01_given_cli_query_then_stdout_displays_selected_geography() {
    let output = cli().arg("place:4159000").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let html = String::from_utf8(output.stdout).unwrap();
    assert!(html.contains("place:4159000"));
    assert!(html.contains("Portland city"));
    assert_eq!(html.matches("<svg").count(), 1);
}

#[test]
fn e02_s01_given_cli_output_path_then_html_is_written_and_io_failures_are_explicit() {
    let directory = TempDirectory::new();
    let destination = directory.0.join("boundary.html");
    let output = cli()
        .arg("cbsa:38900")
        .arg("--output")
        .arg(&destination)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let html = std::fs::read_to_string(destination).unwrap();
    assert!(html.contains("cbsa:38900"));
    assert_eq!(html.matches("<svg").count(), 1);
    let failure = cli()
        .arg("Portland")
        .arg("--output")
        .arg(&directory.0)
        .output()
        .unwrap();
    assert!(!failure.status.success());
    assert!(!failure.stderr.is_empty());
}

#[test]
fn e02_s01_given_invalid_cli_inputs_then_process_reports_failure() {
    for args in [
        vec![],
        vec!["   "],
        vec!["Portland\nOR"],
        vec!["Portland", "--unknown"],
        vec!["Portland", "--output"],
    ] {
        let output = cli().args(&args).output().unwrap();
        assert!(!output.status.success(), "unexpected success for {args:?}");
        assert!(
            !output.stderr.is_empty(),
            "missing actionable error for {args:?}"
        );
    }
}
