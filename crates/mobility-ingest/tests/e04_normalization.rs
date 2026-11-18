use chrono::NaiveDate;
use mobility_ingest::connectors::normalization::{build, load, rebuild, service_instant, Recipe};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/normalization")
}

fn recipe() -> Recipe {
    serde_json::from_slice(&fs::read(fixtures().join("recipe.json")).unwrap()).unwrap()
}

#[test]
fn e04_s01_and_s05_given_explicit_units_and_local_times_when_derived_then_sources_and_outliers_survive(
) {
    let store = tempfile::tempdir().unwrap();
    let id = build(recipe(), &fixtures(), store.path()).unwrap();
    let snap = load(store.path(), &id).unwrap();
    assert_eq!(snap.crs, "EPSG:4326");
    assert_eq!(snap.axis_order, ["longitude", "latitude"]);
    let gtfs = &snap.layers[1].normalized;
    assert_eq!(gtfs["service_times"]["timezone"], "America/Los_Angeles");
    assert_eq!(
        gtfs["service_times"]["tables"]["stop_times.txt"][0]["seconds"]["departure_time"],
        90120
    );
    let spring = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
    let autumn = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
    assert_eq!(
        service_instant("America/Los_Angeles", spring, 0)
            .unwrap()
            .to_rfc3339(),
        "2026-03-08T07:00:00+00:00"
    );
    assert_eq!(
        service_instant("America/Los_Angeles", autumn, 0)
            .unwrap()
            .to_rfc3339(),
        "2026-11-01T08:00:00+00:00"
    );
    assert_eq!(
        service_instant("America/Los_Angeles", spring, 90120)
            .unwrap()
            .to_rfc3339(),
        "2026-03-09T08:02:00+00:00"
    );
    assert!(service_instant("Imaginary/City", spring, 0).is_err());
    let dot = &snap.layers[2];
    assert_eq!(
        dot.normalized["rows"][0]["timestamp_utc"],
        "2026-03-08T09:00:00+00:00"
    );
    assert_eq!(dot.normalized["rows"][0]["unit"], "m/s");
    assert!((dot.normalized["rows"][0]["value"].as_f64().unwrap() - 4.4704).abs() < 1e-10);
    assert_eq!(dot.normalized["rows"][2]["value"], 55.0);
    assert_eq!(dot.findings.len(), 1);
    assert_eq!(dot.source.data["rows"][3]["value"], 200.0);
    assert_eq!(dot.source.data["rows"][3]["unit"], "km/h");
    assert_eq!(dot.source.data["rows"][3]["basis"], "observed");
    assert!(snap.layers[3].normalized.is_null());
    assert!(snap.layers[3].source.report.errors[0]
        .message
        .contains("WGS84"));
}

#[test]
fn e04_s02_and_s03_given_grade_separation_and_walk_entries_when_screened_then_links_are_auditable()
{
    let store = tempfile::tempdir().unwrap();
    let id = build(recipe(), &fixtures(), store.path()).unwrap();
    let snap = load(store.path(), &id).unwrap();
    let osm = &snap.layers[0];
    let topology = &osm.normalized;
    assert_eq!(topology["aliases"]["8"], 7);
    assert_eq!(topology["components"]["6"], topology["components"]["9"]);
    assert_ne!(topology["components"]["9"], topology["components"]["10"]);
    let edge = &topology["edges"][0];
    assert!((edge["length_m"].as_f64().unwrap() - 7.8627).abs() < 0.002);
    for code in [
        "duplicate_edge",
        "impossible_edge",
        "dangling_link",
        "disconnected_component",
        "lanes_review",
        "speed_review",
    ] {
        assert!(osm.findings.iter().any(|f| f.code == code), "{code}");
    }
    assert_eq!(osm.source.data["ways"]["5"]["tags"]["maxspeed"], "300 mph");
    assert_eq!(osm.source.data["ways"]["5"]["tags"]["lanes"], "0");
    let stops = &snap.layers[1].normalized["stop_access"];
    assert_eq!(stops[0]["target"]["node"], 1);
    assert_eq!(stops[1]["status"], "reviewed");
    assert_eq!(stops[1]["review"]["before_node"], Value::Null);
    assert_eq!(stops[1]["review"]["after_node"], 2);
    assert!(snap.recipe.stop_fixes[0].evidence.contains("survey"));
    assert!(
        stops[2]["target"].is_null(),
        "coincident but isolated footway"
    );
    let mut stale = recipe();
    stale.stop_fixes[0].network_version = "stale".into();
    assert!(build(stale, &fixtures(), store.path())
        .unwrap_err()
        .to_string()
        .contains("matching source versions"));
}

#[test]
fn e04_s04_and_s06_given_zero_missing_and_rejected_when_replayed_then_coverage_and_hashes_are_exact(
) {
    let store = tempfile::tempdir().unwrap();
    let id = build(recipe(), &fixtures(), store.path()).unwrap();
    let snap = load(store.path(), &id).unwrap();
    assert_eq!(snap.quality["dot"].missing_regions, ["not-surveyed"]);
    assert_eq!(snap.quality["dot"].completeness, 0.5);
    assert_eq!(snap.quality["places"].rejected_regions, ["pilot"]);
    assert_eq!(snap.quality["places"].invalid_findings, 1);
    assert_eq!(snap.quality["places"].confidence, "unavailable");
    assert_eq!(snap.layers[2].normalized["rows"][1]["value"], 0);
    assert_eq!(snap.raw.iter().filter(|f| f.input == 1).count(), 6);
    assert_eq!(id, build(recipe(), &fixtures(), store.path()).unwrap());
    rebuild(store.path(), &id).unwrap(); // No original input root argument.
    let mut changed = recipe();
    changed.as_of = "2026-03-10".into();
    assert_ne!(id, build(changed, &fixtures(), store.path()).unwrap());
    let raw = &snap.raw[0];
    let path = store.path().join("raw").join(&raw.sha256);
    fs::write(&path, vec![0; raw.bytes as usize]).unwrap();
    assert!(rebuild(store.path(), &id)
        .unwrap_err()
        .to_string()
        .contains("raw SHA-256"));
    assert!(build(recipe(), &fixtures(), store.path())
        .unwrap_err()
        .to_string()
        .contains("immutable object conflict"));
    assert!(load(store.path(), "../../not-a-digest").is_err());
}
