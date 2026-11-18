//! E05-S05 byte-level compatibility fixture, independently assembled from the
//! documented v1 field order using Python's struct.pack, not the Rust encoder.
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
#[test]
fn e05_s05_schema_v1_golden_bytes() {
    let hex = include_str!("fixtures/e05-v1.hex").trim();
    let expected: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let n = Network::new(NetworkData {
        revision: "golden-v1".into(),
        provenance: Provenance {
            source: "synthetic".into(),
            source_version: "v1".into(),
            license: "MIT".into(),
            geography: "abstract".into(),
            date: "2026-11-17".into(),
            basis: Basis::Modeled,
            uncertainty: "uncalibrated".into(),
        },
        timezone: "UTC".into(),
        tzdb: "fixture".into(),
        coverage_start: 0,
        coverage_end: 1000,
        nodes: vec![
            Node {
                id: NodeId(0),
                access: Access::Accessible,
            },
            Node {
                id: NodeId(1),
                access: Access::Accessible,
            },
        ],
        links: vec![Link {
            id: EdgeId(7),
            from: NodeId(0),
            to: NodeId(1),
            distance_m: Some(1.0),
            seconds: 2,
            access: Access::Accessible,
            kind: LinkKind::Street {
                mode: Mode::Walk,
                crossing: false,
                blocked: false,
                cycling: CycleFacility::Unspecified,
                safety: None,
                capacity: None,
            },
        }],
        turns: vec![],
        calendars: vec![],
        days: vec![],
    })
    .unwrap();
    assert_eq!(n.encode(), expected);
    assert_eq!(Network::decode(&expected).unwrap(), n);
    // Malformed tags / boolean are not accepted, even if structurally complete.
    let mut bad = expected.clone();
    bad[208] = 255; // LinkKind tag
    assert!(Network::decode(&bad).is_err());
    let mut bad = expected.clone();
    bad[210] = 255; // crossing boolean
    assert!(Network::decode(&bad).is_err());
    let mut reordered = expected.clone();
    reordered[164..169].copy_from_slice(&expected[169..174]);
    reordered[169..174].copy_from_slice(&expected[164..169]);
    assert!(Network::decode(&reordered).is_err());
    // Deterministic corruption sweep: errors or valid graphs, never a panic.
    for index in 0..expected.len() {
        for byte in [0, 255] {
            let mut mutated = expected.clone();
            mutated[index] = byte;
            let _ = Network::decode(&mutated);
        }
    }
}
