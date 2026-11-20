//! E20-S01/S02: actual normalization, network validation, routing and scoring.
use accessibility::reachable_share;
use mobility_graph::{
    network::{
        Access, Basis, CycleFacility, Link, LinkKind, Network, NetworkData, Node, Provenance,
    },
    EdgeId, Mode, NodeId,
};
use mobility_ingest::connectors::normalization::{build, load, rebuild, Recipe};
use mobility_routing::{earliest_arrival, Outcome, Profile, Request};
use std::{fs, path::Path};

#[derive(serde::Deserialize)]
struct Segment {
    way: i64,
    from: i64,
    to: i64,
    length_m: f64,
}

fn fixture_network(store: &Path) -> Network {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/normalization");
    let recipe: Recipe =
        serde_json::from_slice(&fs::read(fixture.join("recipe.json")).unwrap()).unwrap();
    let id = build(recipe, &fixture, store).unwrap();
    rebuild(store, &id).unwrap();
    let snapshot = load(store, &id).unwrap();
    assert_eq!(snapshot.crs, "EPSG:4326");
    let segments: Vec<Segment> =
        serde_json::from_value(snapshot.layers[0].normalized["edges"].clone()).unwrap();

    // Way 1 is the reviewed, bidirectional fixture route. Model speed at 1 m/s.
    // Node 4 is an explicitly isolated destination. These access labels apply
    // only to this synthetic fixture and are not taken from OSM.
    let links: Vec<_> = segments
        .iter()
        .filter(|edge| edge.way == 1)
        .flat_map(|edge| [(edge, edge.from, edge.to), (edge, edge.to, edge.from)])
        .enumerate()
        .map(|(index, (edge, from, to))| Link {
            id: EdgeId(u32::try_from(index).unwrap()),
            from: NodeId(u32::try_from(from).unwrap()),
            to: NodeId(u32::try_from(to).unwrap()),
            distance_m: Some(edge.length_m),
            seconds: edge.length_m.ceil() as u32,
            access: Access::Accessible,
            kind: LinkKind::Street {
                mode: Mode::Walk,
                crossing: false,
                blocked: false,
                cycling: CycleFacility::Unspecified,
                safety: None,
                capacity: None,
            },
        })
        .collect();
    assert_eq!(links.len(), 4);
    assert!((links[0].distance_m.unwrap() - 7.8627).abs() < 0.002);
    Network::new(NetworkData {
        revision: id,
        provenance: Provenance {
            source: "E04 normalization, fixture way 1".into(),
            source_version: snapshot.processing_version,
            license: "MIT".into(),
            geography: "Synthetic pilot rectangle".into(),
            date: "2026-03-09".into(),
            basis: Basis::Modeled,
            uncertainty: "Fixed 1 m/s modeled speed and explicit fixture access".into(),
        },
        timezone: "UTC".into(),
        tzdb: "fixture".into(),
        coverage_start: 0,
        coverage_end: 120,
        nodes: (1..=4)
            .map(|id| Node {
                id: NodeId(id),
                access: Access::Accessible,
            })
            .collect(),
        links,
        turns: vec![],
        calendars: vec![],
        days: vec![],
    })
    .unwrap()
}

fn reachable(network: &Network, origin: u32, destination: u32, seconds: i64) -> bool {
    let request = Request {
        from: NodeId(origin),
        to: NodeId(destination),
        departure_utc: 0,
        deadline_utc: seconds,
        initial_mode: Mode::Walk,
        vehicle: None,
        profile: Profile {
            allowed_modes: vec![Mode::Walk],
            step_free: true,
            mobility_aid: true,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
        max_states: 1000,
    };
    match earliest_arrival(network, &request).unwrap() {
        Outcome::Journey(journey) => {
            assert!(journey.arrival_utc <= seconds);
            true
        }
        Outcome::Unmet(_) => false,
    }
}

#[test]
fn e20_s01_s02_given_imported_fixture_when_walk_link_added_then_access_replays_and_is_monotone() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let baseline = fixture_network(first.path());
    assert_eq!(baseline.encode(), fixture_network(second.path()).encode());

    let mut data = baseline.data().clone();
    data.revision.push_str("-connector");
    let mut link = data.links[0].clone();
    link.id = EdgeId(4);
    link.from = NodeId(3);
    link.to = NodeId(4);
    data.links.push(link);
    let scenario = Network::new(data).unwrap();
    let mut previous = 0;
    for seconds in 1..=60 {
        let count = (2..=4)
            .filter(|&destination| reachable(&scenario, 1, destination, seconds))
            .count();
        let baseline_count = (2..=4)
            .filter(|&destination| {
                let found = reachable(&baseline, 1, destination, seconds);
                assert!(!found || reachable(&scenario, 1, destination, seconds));
                found
            })
            .count();
        assert!(count >= previous && count >= baseline_count);
        previous = count;
    }
    let score = |network| {
        let count = (2..=4)
            .filter(|&destination| reachable(network, 1, destination, 60))
            .count();
        reachable_share(count, 3)
    };
    assert_eq!(score(&baseline), Some(2.0 / 3.0));
    assert_eq!(score(&scenario), Some(1.0));
    assert!(!reachable(&scenario, 4, 1, 60));
    assert!(reachable(&scenario, 3, 1, 60));
}
