//! E02-S01 Given/When/Then acceptance tests. Synthetic polygons below exercise
//! the format only; they are not observed boundaries or planning inputs.
use mobility_ingest::geography::{Catalogue, GeographyKind, SearchError};

const SYNTHETIC: &str = "carfree-geography-v1\narea\tplace:0000001\tcity\tSynthetic city\tOR\thttps://example.org/source\t2024\t2026-11-17\tSynthetic test data\nring\t-123,45\t-122,45\t-122,46\t-123,45\n";

#[test]
fn e02_s01_given_pilot_when_searching_ambiguous_name_then_all_canonical_candidates_return() {
    let catalogue = Catalogue::pilot().expect("valid embedded Census catalogue");
    let results = catalogue.search("  pOrTlAnD  ").unwrap();
    let ids: Vec<_> = results.iter().map(|area| area.id.as_str()).collect();
    assert_eq!(ids, ["cbsa:38900", "place:2360545", "place:4159000"]);
    assert_eq!(
        catalogue.search("Portland ME").unwrap()[0].id,
        "place:2360545"
    );
    assert_eq!(catalogue.search("Portland OR").unwrap().len(), 2);
    assert_eq!(
        catalogue
            .search("Portland-Vancouver-Hillsboro, OR-WA")
            .unwrap()[0]
            .id,
        "cbsa:38900"
    );
    assert_eq!(
        catalogue.search("place:4159000").unwrap()[0].name,
        "Portland city"
    );
    assert_eq!(
        catalogue.search("cbsa:38900").unwrap()[0].kind,
        GeographyKind::MetropolitanArea
    );
}

#[test]
fn e02_s01_given_canonical_ids_then_source_boundaries_and_provenance_are_retained() {
    let catalogue = Catalogue::pilot().unwrap();
    assert_eq!(catalogue.entries().len(), 3);
    for (id, name, states, kind, source_file) in [
        (
            "place:2360545",
            "Portland city",
            vec!["ME"],
            GeographyKind::City,
            "cb_2024_23_place_500k.zip",
        ),
        (
            "place:4159000",
            "Portland city",
            vec!["OR"],
            GeographyKind::City,
            "cb_2024_41_place_500k.zip",
        ),
        (
            "cbsa:38900",
            "Portland-Vancouver-Hillsboro, OR-WA Metro Area",
            vec!["OR", "WA"],
            GeographyKind::MetropolitanArea,
            "cb_2024_us_cbsa_500k.zip",
        ),
    ] {
        let area = catalogue.get(id).expect("canonical pilot identifier");
        assert_eq!(area.name, name);
        assert_eq!(area.states, states);
        assert_eq!(area.kind, kind);
        assert_eq!(area.vintage, "2024");
        assert!(!area.retrieved_date.is_empty());
        assert!(!area.license.is_empty());
        assert_eq!(
            area.source_url,
            format!("https://www2.census.gov/geo/tiger/GENZ2024/kml/{source_file}")
        );
        assert!(!area.rings.is_empty());
        assert!(
            area.rings.iter().map(Vec::len).sum::<usize>() > 10,
            "actual cartographic boundary, not a stand-in triangle"
        );
        for ring in &area.rings {
            assert!(ring.len() >= 4);
            assert_eq!(
                (ring[0].longitude, ring[0].latitude),
                (
                    ring[ring.len() - 1].longitude,
                    ring[ring.len() - 1].latitude
                )
            );
            assert!(ring.iter().all(|point| point.longitude.is_finite()
                && point.latitude.is_finite()
                && (-125.0..=-65.0).contains(&point.longitude)
                && (40.0..=50.0).contains(&point.latitude)));
        }
    }
    // Independent oracle extracted from the fixed Census KML source artifacts.
    for (id, ring_lengths, first) in [
        (
            "place:4159000",
            vec![599, 8, 9, 9, 13, 4, 7, 7, 10],
            (-122.836482, 45.606912),
        ),
        (
            "place:2360545",
            vec![7, 16, 20, 31, 16, 107],
            (-70.101227, 43.675468),
        ),
        ("cbsa:38900", vec![1377], (-123.785332, 45.166922)),
    ] {
        let area = catalogue.get(id).unwrap();
        assert_eq!(
            area.rings.iter().map(Vec::len).collect::<Vec<_>>(),
            ring_lengths
        );
        assert_eq!(
            (area.rings[0][0].longitude, area.rings[0][0].latitude),
            first
        );
    }
    assert!(catalogue.get("place:9999999").is_none());
}

#[test]
fn e02_s01_given_unsupported_or_invalid_queries_then_no_fallback_is_reported() {
    let catalogue = Catalogue::pilot().unwrap();
    assert!(catalogue
        .search("Unrepresented city 12345")
        .unwrap()
        .is_empty());
    for query in [
        "",
        "   ",
        "Portland\nOR",
        "Portland\tOR",
        "Portland\0OR",
        &"x".repeat(257),
    ] {
        assert!(
            matches!(catalogue.search(query), Err(SearchError::InvalidQuery)),
            "query {query:?}"
        );
    }
}

#[test]
fn e02_s01_given_custom_catalogue_when_parsing_then_all_rings_are_preserved() {
    let input = format!("{SYNTHETIC}ring\t-122.9,45.1\t-122.8,45.1\t-122.8,45.2\t-122.9,45.1\n");
    let catalogue = Catalogue::from_tsv(&input).unwrap();
    let area = catalogue.get("place:0000001").unwrap();
    assert_eq!(area.rings.len(), 2);
    assert_eq!(area.rings[1][0].longitude, -122.9);
    assert_eq!(area.rings[1][0].latitude, 45.1);
    assert_eq!(catalogue.search("SYNTHETIC, OR").unwrap().len(), 1);
}

#[test]
fn e02_s01_given_malformed_catalogue_then_parser_rejects_it_with_actionable_error() {
    let malformed = [
        SYNTHETIC.replace("carfree-geography-v1", "carfree-geography-v2"),
        "carfree-geography-v1\nring\t0,0\t1,0\t1,1\t0,0\n".to_owned(),
        SYNTHETIC.replace("place:0000001", "place:1"),
        SYNTHETIC.replace("\tcity\t", "\tunknown\t"),
        SYNTHETIC.replace("\tcity\t", "\tmetro\t"),
        SYNTHETIC.replace("\tSynthetic city\t", "\t\t"),
        SYNTHETIC.replace("\tOR\t", "\t\t"),
        SYNTHETIC.replace("https://", "javascript:"),
        SYNTHETIC.replace("\t2024\t", "\t\t"),
        SYNTHETIC.replace("\t2026-11-17\t", "\t\t"),
        SYNTHETIC.replace("\tSynthetic test data", "\t"),
        SYNTHETIC.replace("-123,45", "NaN,45"),
        SYNTHETIC.replace("-123,45", "inf,45"),
        SYNTHETIC.replace("-123,45", "181,45"),
        SYNTHETIC.replace("-123,45", "-123,91"),
        SYNTHETIC.replace("-123,45", "invalid"),
        SYNTHETIC.replace("\t-123,45\n", "\t-123,46\n"),
        SYNTHETIC.replace("\t-122,46", ""),
        SYNTHETIC.replace("-122,45\t-122,46", "-123,45\t-123,45"),
        SYNTHETIC.lines().take(2).collect::<Vec<_>>().join("\n"),
        format!(
            "{SYNTHETIC}{}",
            SYNTHETIC.lines().skip(1).collect::<Vec<_>>().join("\n")
        ),
        SYNTHETIC.replace("\tcity\t", "\tcity\textra\t"),
    ];
    for input in malformed {
        let error = match Catalogue::from_tsv(&input) {
            Ok(_) => panic!("accepted invalid catalogue: {input:?}"),
            Err(error) => error.to_string(),
        };
        assert!(!error.trim().is_empty());
        assert!(
            error.to_ascii_lowercase().contains("line"),
            "line context missing: {error}"
        );
    }
}
