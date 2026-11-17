//! E02-S02 through E02-S05 acceptance scenarios. All custom coordinates and
//! importer responses below are synthetic test inputs, never observations.
use mobility_ingest::geography::{Catalogue, Coordinate};
use mobility_ingest::study_area::{
    CatalogueImporter, CityProject, CoverageEntry, CoverageReport, DatasetKind, DatasetStatus,
    Importer, OnboardingError, Region, StudyArea,
};

fn point(longitude: f64, latitude: f64) -> Coordinate {
    Coordinate {
        longitude,
        latitude,
    }
}
fn ring(points: &[(f64, f64)]) -> Vec<Coordinate> {
    points.iter().map(|&(x, y)| point(x, y)).collect()
}
fn rectangle(west: f64, south: f64, east: f64, north: f64) -> Vec<Coordinate> {
    ring(&[
        (west, south),
        (east, south),
        (east, north),
        (west, north),
        (west, south),
    ])
}
fn region() -> Region {
    Region {
        country: "US".into(),
        states: vec!["OR".into()],
    }
}
fn drawn(rings: Vec<Vec<Coordinate>>) -> Result<StudyArea, OnboardingError> {
    StudyArea::drawn(
        "synthetic-core",
        "Synthetic test core",
        region(),
        "EPSG:4326",
        rings,
    )
}
fn core() -> StudyArea {
    drawn(vec![rectangle(-123.0, 45.0, -122.9, 45.1)]).unwrap()
}
fn missing_report() -> CoverageReport {
    let kinds = [
        DatasetKind::Boundary,
        DatasetKind::Roads,
        DatasetKind::Transit,
        DatasetKind::Population,
        DatasetKind::Demand,
    ];
    CoverageReport {
        entries: kinds
            .iter()
            .map(|&kind| CoverageEntry {
                kind,
                source: None,
                version: None,
                year: None,
                extent: None,
                status: DatasetStatus::Missing,
                detail: "Synthetic test: no source data provided".into(),
            })
            .collect(),
        missing_required: kinds.to_vec(),
    }
}

#[derive(Default)]
struct CountingImporter {
    calls: Vec<(String, Vec<Vec<Coordinate>>)>,
    fail: bool,
}
impl Importer for CountingImporter {
    fn ingest(
        &mut self,
        area: &StudyArea,
        snapshot_date: &str,
    ) -> Result<CoverageReport, OnboardingError> {
        self.calls
            .push((snapshot_date.into(), area.rings().to_vec()));
        if self.fail {
            return match StudyArea::drawn("invalid", "Synthetic", region(), "invalid-crs", vec![]) {
                Err(error) => Err(error),
                Ok(_) => panic!("invalid CRS unexpectedly accepted"),
            };
        }
        Ok(missing_report())
    }
}

#[test]
fn e02_s02_given_canonical_city_or_metro_when_selected_then_exact_source_geometry_and_region_survive(
) {
    let catalogue = Catalogue::pilot().unwrap();
    for (id, states, rings) in [
        ("place:2360545", vec!["ME"], 6),
        ("place:4159000", vec!["OR"], 9),
        ("cbsa:38900", vec!["OR", "WA"], 1),
    ] {
        let geography = catalogue.get(id).unwrap();
        let area = StudyArea::from_geography(geography).unwrap();
        assert_eq!(area.id(), id);
        assert_eq!(area.name(), geography.name);
        assert_eq!(area.crs(), "EPSG:4326");
        assert_eq!(area.region().country, "US");
        assert_eq!(area.region().states, states);
        assert_eq!(area.rings(), geography.rings);
        assert_eq!(area.core_rings(), geography.rings);
        assert_eq!(area.rings().len(), rings);
    }
}

#[test]
fn e02_s02_given_shell_hole_and_island_when_saved_then_valid_topology_is_retained() {
    let rings = vec![
        rectangle(-123.0, 45.0, -122.0, 46.0),
        rectangle(-122.8, 45.2, -122.2, 45.8),
        rectangle(-121.9, 45.0, -121.8, 45.1),
        rectangle(-122.6, 45.4, -122.4, 45.6), // island nested inside the hole
    ];
    let area = drawn(rings.clone()).unwrap();
    assert_eq!(area.rings(), rings);
    let mut importer = CountingImporter::default();
    let project =
        CityProject::initialize("synthetic-project", area, "2024-02-29", &mut importer).unwrap();
    let saved = project.to_tsv();
    assert!(saved.starts_with("carfree-study-project-v1\n"));
    let restored = CityProject::from_tsv(&saved).unwrap();
    assert_eq!(restored.area().rings(), rings);
    assert_eq!(restored.area().crs(), "EPSG:4326");
    assert_eq!(restored.area().region().states, ["OR"]);
    assert_eq!(restored.to_tsv(), saved);
}

#[test]
fn e02_s02_given_invalid_geometry_when_drawn_then_actionable_error_prevents_save() {
    let bad = vec![
        vec![],
        vec![ring(&[(-123.0, 45.0), (-122.0, 45.0), (-122.0, 46.0)])],
        vec![ring(&[
            (-123.0, 45.0),
            (-122.0, 46.0),
            (-123.0, 46.0),
            (-122.0, 45.0),
            (-123.0, 45.0),
        ])],
        vec![ring(&[
            (-123.0, 45.0),
            (-122.0, 46.0),
            (-121.0, 47.0),
            (-123.0, 45.0),
        ])],
        vec![rectangle(-123.0, 45.0, f64::NAN, 46.0)],
        vec![rectangle(-123.0, 45.0, f64::INFINITY, 46.0)],
        vec![rectangle(-181.0, 45.0, -122.0, 46.0)],
        vec![rectangle(-123.0, 45.0, -122.0, 91.0)],
        vec![
            rectangle(-123.0, 45.0, -122.0, 46.0),
            rectangle(-122.5, 45.5, -121.5, 46.5),
        ],
        vec![
            rectangle(-123.0, 45.0, -122.0, 46.0),
            rectangle(-122.0, 45.2, -121.5, 45.8),
        ],
        vec![
            rectangle(-123.0, 45.0, -122.0, 46.0),
            rectangle(-123.0, 45.0, -122.0, 46.0),
        ],
    ];
    for rings in bad {
        match drawn(rings) {
            Err(error) => assert!(!error.to_string().trim().is_empty()),
            Ok(_) => panic!("invalid geometry accepted"),
        }
    }
    for (id, name, country, states, crs) in [
        ("../escape", "Synthetic", "US", vec!["OR"], "EPSG:4326"),
        ("synthetic", "", "US", vec!["OR"], "EPSG:4326"),
        ("synthetic", "bad\tmetadata", "US", vec!["OR"], "EPSG:4326"),
        ("synthetic", "Synthetic", "", vec!["OR"], "EPSG:4326"),
        ("synthetic", "Synthetic", "US", vec![], "EPSG:4326"),
        ("synthetic", "Synthetic", "US", vec!["OR"], "EPSG:3857"),
    ] {
        let region = Region {
            country: country.into(),
            states: states.into_iter().map(str::to_owned).collect(),
        };
        assert!(StudyArea::drawn(
            id,
            name,
            region,
            crs,
            vec![rectangle(-123.0, 45.0, -122.0, 46.0)]
        )
        .is_err());
    }
}

#[test]
fn e02_s03_given_pilot_when_previewing_coverage_then_real_boundary_and_missing_transport_are_explicit(
) {
    let catalogue = Catalogue::pilot().unwrap();
    let area = StudyArea::from_geography(catalogue.get("place:4159000").unwrap()).unwrap();
    let mut importer = CatalogueImporter::new(&catalogue);
    let report = importer.ingest(&area, "2026-11-17").unwrap();
    assert_eq!(report.entries.len(), 5);
    let boundary = report
        .entries
        .iter()
        .find(|e| e.kind == DatasetKind::Boundary)
        .unwrap();
    assert_eq!(boundary.status, DatasetStatus::Available);
    assert_eq!(boundary.year, Some(2024));
    assert!(boundary.source.as_deref().unwrap().contains("census.gov"));
    assert!(boundary
        .version
        .as_deref()
        .is_some_and(|v| v.contains("2024")));
    let extent = boundary.extent.unwrap();
    assert!(extent[0] < -122.8 && extent[2] > -122.5);
    assert!(extent[1] < 45.5 && extent[3] > 45.6);
    for kind in [
        DatasetKind::Roads,
        DatasetKind::Transit,
        DatasetKind::Population,
        DatasetKind::Demand,
    ] {
        let entry = report.entries.iter().find(|e| e.kind == kind).unwrap();
        assert_eq!(entry.status, DatasetStatus::Missing);
        assert!(entry.source.is_none());
        assert!(entry.version.is_none());
        assert!(entry.year.is_none());
        assert!(!entry.detail.is_empty());
        assert!(report.missing_required.contains(&kind));
    }
}

#[test]
fn e02_s03_given_snapshot_before_source_vintage_then_future_data_is_not_available() {
    let catalogue = Catalogue::pilot().unwrap();
    let area = StudyArea::from_geography(catalogue.get("cbsa:38900").unwrap()).unwrap();
    let report = CatalogueImporter::new(&catalogue)
        .ingest(&area, "2023-12-31")
        .unwrap();
    let boundary = report
        .entries
        .iter()
        .find(|e| e.kind == DatasetKind::Boundary)
        .unwrap();
    assert_ne!(boundary.status, DatasetStatus::Available);
    assert!(report.missing_required.contains(&DatasetKind::Boundary));
}

#[test]
fn e02_s04_given_snapshot_when_initialized_then_full_project_is_durable_and_reproducible() {
    let catalogue = Catalogue::pilot().unwrap();
    let area = StudyArea::from_geography(catalogue.get("place:2360545").unwrap()).unwrap();
    let mut importer = CatalogueImporter::new(&catalogue);
    let mut project =
        CityProject::initialize("portland-me", area, "2026-11-17", &mut importer).unwrap();
    assert_eq!(project.id(), "portland-me");
    assert_eq!(project.snapshot_date(), "2026-11-17");
    assert!(!project.baseline_id().is_empty());
    assert_eq!(project.import_attempts(), 1);
    project.add_scenario("reference-case").unwrap();
    assert_eq!(project.scenarios().len(), 1);
    assert!(!project.scenarios()[0].stale);
    assert_eq!(project.scenarios()[0].baseline_id, project.baseline_id());
    let encoded = project.to_tsv();
    let restored = CityProject::from_tsv(&encoded).unwrap();
    assert_eq!(restored.id(), "portland-me");
    assert_eq!(restored.snapshot_date(), "2026-11-17");
    assert_eq!(restored.baseline_id(), project.baseline_id());
    assert_eq!(restored.area().rings(), project.area().rings());
    assert_eq!(restored.coverage().missing_required.len(), 4);
    assert_eq!(restored.scenarios()[0].id, "reference-case");
    assert_eq!(restored.to_tsv(), encoded);
    assert!(project.add_scenario("reference-case").is_err());
    for malformed in [
        encoded.replacen("carfree-study-project-v1", "carfree-study-project-v999", 1),
        encoded.replace("2026-11-17", "2026-02-30"),
        "carfree-study-project-v1\n".into(),
        format!("{encoded}unknown\tignored?\n"),
    ] {
        assert!(
            CityProject::from_tsv(&malformed).is_err(),
            "malformed persistence accepted"
        );
    }
}

#[test]
fn e02_s04_given_invalid_dates_or_project_ids_then_initialization_fails_before_ingestion() {
    for date in [
        "2023-02-29",
        "2024-02-30",
        "2026-00-01",
        "2026-13-01",
        "2026-11-00",
        "2026-11-31",
        "2026-1-01",
        "tomorrow",
    ] {
        let mut importer = CountingImporter::default();
        assert!(
            CityProject::initialize("valid", core(), date, &mut importer).is_err(),
            "accepted {date}"
        );
        assert!(importer.calls.is_empty());
    }
    for id in ["", "../escape", "/absolute", "bad\tfield", "bad\nrow"] {
        let mut importer = CountingImporter::default();
        assert!(CityProject::initialize(id, core(), "2024-02-29", &mut importer).is_err());
        assert!(importer.calls.is_empty());
    }
}

#[test]
fn e02_s05_given_existing_scenario_when_buffer_expands_then_ingestion_reruns_and_staleness_persists(
) {
    let mut importer = CountingImporter::default();
    let mut project =
        CityProject::initialize("buffer-test", core(), "2024-02-29", &mut importer).unwrap();
    project.add_scenario("before-expansion").unwrap();
    let baseline = project.baseline_id().to_owned();
    let revision = project.revision();
    let core_rings = project.area().core_rings().to_vec();
    project.expand_buffer(5.0, &mut importer).unwrap();
    assert_eq!(
        importer.calls.len(),
        2,
        "must invoke importer again, not only enqueue work"
    );
    assert_eq!(importer.calls[1].0, "2024-02-29");
    assert_eq!(importer.calls[1].1, project.area().rings());
    let expanded = &project.area().rings()[0];
    let min_lon = expanded
        .iter()
        .map(|p| p.longitude)
        .fold(f64::INFINITY, f64::min);
    let max_lon = expanded
        .iter()
        .map(|p| p.longitude)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_lat = expanded
        .iter()
        .map(|p| p.latitude)
        .fold(f64::INFINITY, f64::min);
    let max_lat = expanded
        .iter()
        .map(|p| p.latitude)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(min_lon < -123.04 && max_lon > -122.86);
    assert!(min_lat < 44.96 && max_lat > 45.14);
    assert_eq!(project.area().core_rings(), core_rings);
    assert_eq!(project.revision(), revision + 1);
    assert_eq!(project.import_attempts(), 2);
    assert_ne!(project.baseline_id(), baseline);
    assert!(project.scenarios()[0].stale);
    assert_eq!(project.scenarios()[0].baseline_id, baseline);
    let saved = project.to_tsv();
    let restored = CityProject::from_tsv(&saved).unwrap();
    assert!(restored.scenarios()[0].stale);
    assert_eq!(restored.area().core_rings(), core_rings);
    assert_eq!(restored.to_tsv(), saved);
    project.expand_buffer(5.0, &mut importer).unwrap();
    assert_eq!(
        importer.calls.len(),
        2,
        "same requested buffer is idempotent"
    );
    assert_eq!(project.to_tsv(), saved);
}

#[test]
fn e02_s05_given_import_failure_when_expanding_then_no_partial_revision_or_scenario_mutation_occurs(
) {
    let mut importer = CountingImporter::default();
    let mut project =
        CityProject::initialize("atomic-test", core(), "2024-02-29", &mut importer).unwrap();
    project.add_scenario("prior").unwrap();
    let before = project.to_tsv();
    importer.fail = true;
    assert!(project.expand_buffer(5.0, &mut importer).is_err());
    assert_eq!(importer.calls.len(), 2);
    assert_eq!(project.to_tsv(), before);
    for km in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(project.expand_buffer(km, &mut importer).is_err());
        assert_eq!(project.to_tsv(), before);
    }
}

#[test]
fn e02_s04_given_identical_inputs_then_baseline_is_deterministic_but_changed_snapshot_is_distinct()
{
    let mut a = CountingImporter::default();
    let mut b = CountingImporter::default();
    let first = CityProject::initialize("same-inputs", core(), "2024-02-29", &mut a).unwrap();
    let repeated = CityProject::initialize("same-inputs", core(), "2024-02-29", &mut b).unwrap();
    assert_eq!(first.baseline_id(), repeated.baseline_id());
    assert_eq!(first.to_tsv(), repeated.to_tsv());
    let changed = CityProject::initialize("same-inputs", core(), "2024-03-01", &mut b).unwrap();
    assert_ne!(first.baseline_id(), changed.baseline_id());
}

#[test]
fn e02_s05_given_buffer_crossing_pole_or_dateline_then_unsupported_envelope_fails_explicitly() {
    for (area, km) in [
        (
            drawn(vec![rectangle(179.8, 45.0, 179.9, 45.1)]).unwrap(),
            50.0,
        ),
        (
            drawn(vec![rectangle(-10.0, 89.8, -9.9, 89.9)]).unwrap(),
            50.0,
        ),
    ] {
        assert!(area.buffer_envelope(km).is_err());
    }
}

#[test]
fn e02_s05_given_expanded_metro_when_reingested_then_source_extent_does_not_falsely_cover_buffer() {
    let catalogue = Catalogue::pilot().unwrap();
    let area = StudyArea::from_geography(catalogue.get("cbsa:38900").unwrap()).unwrap();
    let mut importer = CatalogueImporter::new(&catalogue);
    let mut project =
        CityProject::initialize("expanded-coverage", area, "2026-11-17", &mut importer).unwrap();
    project.expand_buffer(10.0, &mut importer).unwrap();
    let boundary = project
        .coverage()
        .entries
        .iter()
        .find(|entry| entry.kind == DatasetKind::Boundary)
        .unwrap();
    assert_ne!(boundary.status, DatasetStatus::Available);
    assert!(project
        .coverage()
        .missing_required
        .contains(&DatasetKind::Boundary));
    assert_eq!(project.import_attempts(), 2);
}

#[test]
fn e02_s02_given_nearly_collinear_nonzero_area_then_geometry_is_not_silently_snapped_away() {
    // Synthetic thin triangle has known positive area 1e-12 square degrees.
    // It is deliberately much thinner than ordinary map rendering resolution.
    let vertices = ring(&[(0.0, 0.0), (1.0, 1e-12), (2.0, 0.0), (0.0, 0.0)]);
    let area = drawn(vec![vertices.clone()]).unwrap();
    assert_eq!(area.rings(), [vertices]);
}

#[test]
fn e02_s02_given_isolated_hole_tangency_then_valid_polygon_is_supported_but_disconnected_interior_is_rejected(
) {
    let tangent = vec![
        rectangle(-123.0, 45.0, -122.0, 46.0),
        ring(&[
            (-123.0, 45.0),
            (-122.8, 45.1),
            (-122.9, 45.2),
            (-123.0, 45.0),
        ]),
    ];
    assert!(
        drawn(tangent).is_ok(),
        "single shell/hole point contact has connected interior"
    );
    let disconnecting = vec![
        ring(&[
            (-123.0, 45.0),
            (-122.0, 45.0),
            (-122.0, 45.5),
            (-122.0, 46.0),
            (-123.0, 46.0),
            (-123.0, 45.5),
            (-123.0, 45.0),
        ]),
        ring(&[
            (-123.0, 45.5),
            (-122.5, 45.2),
            (-122.0, 45.5),
            (-122.5, 45.8),
            (-123.0, 45.5),
        ]),
    ];
    assert!(
        drawn(disconnecting).is_err(),
        "hole touching opposite shell sides disconnects polygon interior"
    );
}

/// Change one documented TSV field without recomputing baseline checksums.
/// This exercises the public loader's rejection contract, not its hash algorithm.
fn change_field(input: &str, tag: &str, index: usize, replacement: &str) -> String {
    let mut changed = false;
    let mut output = String::new();
    for line in input.lines() {
        let mut fields: Vec<_> = line.split('\t').collect();
        if fields[0] == tag && !changed {
            fields[index] = replacement;
            changed = true;
        }
        output.push_str(&fields.join("\t"));
        output.push('\n');
    }
    assert!(changed, "fixture must contain record {tag}");
    output
}

#[test]
fn e02_s04_given_corrupted_persisted_inputs_then_reload_rejects_inconsistent_baseline_and_state() {
    let catalogue = Catalogue::pilot().unwrap();
    let area = StudyArea::from_geography(catalogue.get("place:2360545").unwrap()).unwrap();
    let mut importer = CatalogueImporter::new(&catalogue);
    let mut project =
        CityProject::initialize("corruption-test", area, "2026-11-17", &mut importer).unwrap();
    project.add_scenario("current").unwrap();
    let saved = project.to_tsv();
    for (label, malformed) in [
        (
            "future valid snapshot without new baseline",
            change_field(&saved, "project", 2, "2026-11-18"),
        ),
        ("zero revision", change_field(&saved, "project", 3, "0")),
        (
            "attempts disagree with revision",
            change_field(&saved, "project", 4, "2"),
        ),
        (
            "changed original core",
            change_field(&saved, "ring", 1, "-70,44"),
        ),
        (
            "unknown coverage status",
            change_field(&saved, "dataset", 2, "PretendReady"),
        ),
        (
            "status disagrees with missing list",
            change_field(&saved, "dataset", 2, "Missing"),
        ),
        (
            "available source missing version",
            change_field(&saved, "dataset", 4, "-"),
        ),
        (
            "changed source version without new baseline",
            change_field(&saved, "dataset", 4, "=2023"),
        ),
        (
            "reversed coverage extent",
            change_field(&saved, "dataset", 6, "-70,44,-71,43"),
        ),
        (
            "dropped missing required datasets",
            change_field(&saved, "missing", 1, ""),
        ),
        (
            "fresh scenario marked stale",
            change_field(&saved, "scenario", 3, "true"),
        ),
        (
            "fresh scenario references other baseline",
            change_field(&saved, "scenario", 2, "baseline-other-r1"),
        ),
    ] {
        assert!(
            CityProject::from_tsv(&malformed).is_err(),
            "accepted corruption: {label}"
        );
    }
    project.expand_buffer(5.0, &mut importer).unwrap();
    let stale = project.to_tsv();
    assert!(CityProject::from_tsv(&stale).is_ok());
    for (label, malformed) in [
        (
            "old scenario falsely fresh",
            change_field(&stale, "scenario", 3, "false"),
        ),
        (
            "stale scenario references current baseline",
            change_field(&stale, "scenario", 2, project.baseline_id()),
        ),
        (
            "changed buffer without changed geometry",
            change_field(&stale, "area", 6, "10"),
        ),
    ] {
        assert!(
            CityProject::from_tsv(&malformed).is_err(),
            "accepted corruption: {label}"
        );
    }
}

#[test]
fn e02_s03_given_importer_omits_required_dataset_kinds_then_project_cannot_claim_complete_coverage()
{
    struct BoundaryOnlyImporter;
    impl Importer for BoundaryOnlyImporter {
        fn ingest(&mut self, _: &StudyArea, _: &str) -> Result<CoverageReport, OnboardingError> {
            Ok(CoverageReport {
                entries: vec![CoverageEntry {
                    kind: DatasetKind::Boundary,
                    source: Some("Synthetic source".into()),
                    version: Some("synthetic-v1".into()),
                    year: Some(2024),
                    extent: Some([-123.0, 45.0, -122.9, 45.1]),
                    status: DatasetStatus::Available,
                    detail: "Synthetic test: transport sources deliberately omitted".into(),
                }],
                missing_required: vec![],
            })
        }
    }
    assert!(
        CityProject::initialize(
            "omitted-inputs",
            core(),
            "2024-02-29",
            &mut BoundaryOnlyImporter
        )
        .is_err(),
        "absence of a required dataset row cannot mean complete coverage"
    );
}

#[test]
fn e02_s03_given_drawn_area_with_canonical_id_then_ingestion_preserves_user_origin_without_census_attribution(
) {
    let catalogue = Catalogue::pilot().unwrap();
    let area = StudyArea::drawn(
        "place:4159000",
        "Synthetic drawn polygon",
        region(),
        "EPSG:4326",
        vec![rectangle(-123.0, 45.0, -122.9, 45.1)],
    )
    .unwrap();
    let report = CatalogueImporter::new(&catalogue)
        .ingest(&area, "2026-11-17")
        .unwrap();
    let boundary = report
        .entries
        .iter()
        .find(|entry| entry.kind == DatasetKind::Boundary)
        .unwrap();
    assert_eq!(boundary.status, DatasetStatus::Available);
    let source = boundary.source.as_deref().unwrap().to_ascii_lowercase();
    assert!(source.contains("user"));
    assert!(!source.contains("census"));
    assert_eq!(boundary.version.as_deref(), Some("drawn-v1"));
    assert_eq!(boundary.extent, Some([-123.0, 45.0, -122.9, 45.1]));
}

#[test]
fn e02_s03_given_source_backed_area_absent_from_catalogue_then_ingestion_does_not_relabel_it_user_drawn(
) {
    let catalogue = Catalogue::pilot().unwrap();
    let mut source_geography = catalogue.get("place:2360545").unwrap().clone();
    source_geography.id = "place:9999999".into();
    let area = StudyArea::from_geography(&source_geography).unwrap();
    assert!(
        CatalogueImporter::new(&catalogue)
            .ingest(&area, "2026-11-17")
            .is_err(),
        "missing source-backed geography must fail explicitly"
    );
}

#[test]
fn e02_s02_given_malformed_source_dates_then_area_construction_fails_before_success_can_be_persisted(
) {
    let catalogue = Catalogue::pilot().unwrap();
    for (vintage, retrieved) in [
        ("not-a-year", "2026-11-17"),
        ("0000", "2026-11-17"),
        ("2024", "2026-02-30"),
        ("2024", "yesterday"),
    ] {
        let mut geography = catalogue.get("place:2360545").unwrap().clone();
        geography.vintage = vintage.into();
        geography.retrieved_date = retrieved.into();
        assert!(
            StudyArea::from_geography(&geography).is_err(),
            "accepted source vintage {vintage}, retrieved {retrieved}"
        );
    }
}

#[test]
fn e02_s03_given_catalogue_identity_matches_but_source_or_core_differs_then_ingestion_fails_explicitly(
) {
    let catalogue = Catalogue::pilot().unwrap();
    let original = catalogue.get("place:2360545").unwrap();
    let mut wrong_source = original.clone();
    wrong_source.source_url = "https://example.org/synthetic-unmatched-source".into();
    let mut wrong_version = original.clone();
    wrong_version.vintage = "2023".into();
    let mut wrong_core = original.clone();
    for point in wrong_core.rings.iter_mut().flatten() {
        point.longitude += 0.01;
    }
    for geography in [wrong_source, wrong_version, wrong_core] {
        let area = StudyArea::from_geography(&geography).unwrap();
        assert!(
            CatalogueImporter::new(&catalogue)
                .ingest(&area, "2026-11-17")
                .is_err(),
            "ID equality cannot replace verification of pinned source and source geometry"
        );
    }
}

#[test]
fn e02_s04_given_maximum_scenario_count_then_next_addition_fails_without_creating_unloadable_state()
{
    let mut importer = CountingImporter::default();
    let mut project =
        CityProject::initialize("scenario-limit", core(), "2024-02-29", &mut importer).unwrap();
    for index in 0..10_000 {
        project.add_scenario(&format!("s{index}")).unwrap();
    }
    let before = project.to_tsv();
    assert_eq!(
        CityProject::from_tsv(&before).unwrap().scenarios().len(),
        10_000
    );
    assert!(
        project.add_scenario("overflow").is_err(),
        "an accepted operation must not exceed the loader's 10000-scenario limit"
    );
    assert_eq!(project.to_tsv(), before);
    assert_eq!(project.scenarios().len(), 10_000);
}

#[test]
fn e02_s04_given_oversized_initial_metadata_then_no_unreloadable_project_is_accepted() {
    let area = StudyArea::drawn(
        "synthetic-core",
        &"x".repeat(16 * 1024 * 1024),
        region(),
        "EPSG:4326",
        vec![rectangle(-123.0, 45.0, -122.9, 45.1)],
    )
    .unwrap();
    let mut importer = CountingImporter::default();
    assert!(
        CityProject::initialize("initial-byte-limit", area, "2024-02-29", &mut importer).is_err(),
        "full project serialization exceeds the public 16 MiB loader limit"
    );
}

#[test]
fn e02_s04_given_near_size_limit_then_scenario_addition_rejects_overflow_atomically() {
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    const RESERVE: usize = 64;
    let make_area = |name: &str| {
        StudyArea::drawn(
            "synthetic-core",
            name,
            region(),
            "EPSG:4326",
            vec![rectangle(-123.0, 45.0, -122.9, 45.1)],
        )
        .unwrap()
    };
    let mut importer = CountingImporter::default();
    let small = CityProject::initialize(
        "scenario-byte-limit",
        make_area("x"),
        "2024-02-29",
        &mut importer,
    )
    .unwrap();
    // Find the fixture's fixed metadata overhead through public serialization.
    // No checksum or record construction is reproduced in this test.
    let name_length = MAX_BYTES - RESERVE - (small.to_tsv().len() - 1);
    let mut project = CityProject::initialize(
        "scenario-byte-limit",
        make_area(&"x".repeat(name_length)),
        "2024-02-29",
        &mut importer,
    )
    .unwrap();
    let before = project.to_tsv();
    assert_eq!(before.len(), MAX_BYTES - RESERVE);
    assert!(
        CityProject::from_tsv(&before).is_ok(),
        "valid near-limit state must reload"
    );
    assert!(
        project.add_scenario(&"s".repeat(128)).is_err(),
        "valid identifier would make the full schema exceed 16 MiB"
    );
    assert_eq!(project.to_tsv(), before);
    assert!(project.scenarios().is_empty());
}

#[test]
fn e02_s05_given_oversized_reimport_result_then_buffer_change_preserves_reloadable_previous_state()
{
    struct OversizedImporter;
    impl Importer for OversizedImporter {
        fn ingest(&mut self, _: &StudyArea, _: &str) -> Result<CoverageReport, OnboardingError> {
            let mut report = missing_report();
            report.entries[0].detail = "x".repeat(17 * 1024 * 1024);
            Ok(report)
        }
    }
    let mut importer = CountingImporter::default();
    let mut project =
        CityProject::initialize("buffer-byte-limit", core(), "2024-02-29", &mut importer).unwrap();
    project.add_scenario("preserved").unwrap();
    let before = project.to_tsv();
    assert!(
        project.expand_buffer(5.0, &mut OversizedImporter).is_err(),
        "oversized imported metadata must not become accepted project state"
    );
    assert_eq!(project.to_tsv(), before);
    assert!(CityProject::from_tsv(&project.to_tsv()).is_ok());
    assert!(!project.scenarios()[0].stale);
}
