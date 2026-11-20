//! E14-S02 Given/When/Then acceptance tests. All projects are synthetic.
use optimizer_env::catalog::{Catalog, Error, Project, Reason, MAX_PROJECTS};
use std::collections::BTreeSet;

fn ids(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|s| (*s).into()).collect()
}
fn project(id: &str, requires: &[&str], conflicts: &[&str]) -> Project<u32> {
    Project {
        id: id.into(),
        description: format!("Synthetic project {id}"),
        requires: ids(requires),
        conflicts: ids(conflicts),
        payload: 42,
    }
}
fn catalog() -> Catalog<u32> {
    Catalog::new(
        "fixture-v1".into(),
        vec![
            project("walk", &[], &[]),
            project("bus", &["walk"], &[]),
            project("closure", &[], &["walk"]),
        ],
    )
    .unwrap()
}
#[test]
fn e14_s02_masks_prerequisites_and_symmetric_conflicts() {
    let c = catalog();
    let mask = c.mask("state-0", &ids(&[]), |_| Ok(vec![])).unwrap();
    assert_eq!(
        mask.entries.iter().map(|e| e.allowed()).collect::<Vec<_>>(),
        vec![true, false, true]
    );
    assert_eq!(
        mask.entries[1].reasons,
        vec![Reason::MissingPrerequisite("walk".into())]
    );
    let mask = c.mask("state-1", &ids(&["walk"]), |_| Ok(vec![])).unwrap();
    assert_eq!(mask.entries[0].reasons, vec![Reason::AlreadyApplied]);
    assert!(mask.entries[1].allowed());
    assert_eq!(
        mask.entries[2].reasons,
        vec![Reason::ConflictingProject("walk".into())]
    );
    let mask = c
        .mask("state-2", &ids(&["closure"]), |_| Ok(vec![]))
        .unwrap();
    assert_eq!(
        mask.entries[0].reasons,
        vec![Reason::ConflictingProject("closure".into())]
    );
}
#[test]
fn e14_s02_physical_rejection_has_explicit_reasons() {
    let c = catalog();
    let mask = c
        .mask("state-0", &ids(&[]), |p| {
            Ok(if p.id == "walk" {
                vec![
                    "missing width evidence".into(),
                    "capital budget exceeded".into(),
                ]
            } else {
                vec![]
            })
        })
        .unwrap();
    assert_eq!(
        mask.entries[0].reasons,
        vec![
            Reason::Screening("missing width evidence".into()),
            Reason::Screening("capital budget exceeded".into())
        ]
    );
    let error = c
        .resolve(0, "fixture-v1", "state-0", "state-0", &ids(&[]), |_| {
            Ok(vec!["missing width evidence".into()])
        })
        .unwrap_err();
    assert!(error.0.contains("missing width evidence"));
}
#[test]
fn e14_s02_resolution_revalidates_instead_of_trusting_masks() {
    let c = catalog();
    let mut mask = c.mask("state-0", &ids(&[]), |_| Ok(vec![])).unwrap();
    mask.entries[1].reasons.clear(); // Forged caller mask does not authorize bus.
    let error = c
        .resolve(
            1,
            &mask.catalog_revision,
            &mask.state_revision,
            "state-0",
            &ids(&[]),
            |_| Ok(vec![]),
        )
        .unwrap_err();
    assert!(error.0.contains("MissingPrerequisite"));
    assert_eq!(
        c.resolve(
            1,
            "fixture-v1",
            "state-1",
            "state-1",
            &ids(&["walk"]),
            |_| Ok(vec![])
        )
        .unwrap()
        .payload,
        42
    );
    assert!(c
        .resolve(
            1,
            "fixture-v1",
            "state-1",
            "state-2",
            &ids(&["walk"]),
            |_| panic!("stale actions must not screen")
        )
        .unwrap_err()
        .0
        .contains("stale"));
    assert!(c
        .resolve(
            0,
            "different-catalog",
            "state-0",
            "state-0",
            &ids(&[]),
            |_| panic!("stale catalog must not screen")
        )
        .is_err());
    assert!(c
        .resolve(
            99,
            "fixture-v1",
            "state-0",
            "state-0",
            &ids(&[]),
            |_| panic!("unknown action must not screen")
        )
        .unwrap_err()
        .0
        .contains("nonexistent"));
}
#[test]
fn e14_s02_screen_errors_are_not_hidden_as_infeasibility() {
    let c = catalog();
    let error = c
        .mask("state-0", &ids(&[]), |_| {
            Err(Error("routing resource exhausted".into()))
        })
        .unwrap_err();
    assert_eq!(error.0, "routing resource exhausted");
    assert!(c
        .mask("state-0", &ids(&[]), |_| Ok(vec!["".into()]))
        .is_err());
    assert!(c
        .mask("state-0", &ids(&[]), |_| Ok(vec!["bad\nreason".into()]))
        .is_err());
}
#[test]
fn e14_s02_structural_rejections_do_not_invoke_expensive_screening() {
    let c = catalog();
    let mut called = vec![];
    c.mask("state-1", &ids(&["walk"]), |p| {
        called.push(p.id.clone());
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(called, vec!["bus"]);
}
#[test]
fn e14_s02_invalid_applied_states_fail_explicitly() {
    for applied in [ids(&["unknown"]), ids(&["bus"]), ids(&["walk", "closure"])] {
        assert!(catalog()
            .mask("state-0", &applied, |_| panic!(
                "invalid states must not screen"
            ))
            .is_err());
    }
    assert!(catalog().mask("", &ids(&[]), |_| Ok(vec![])).is_err());
}
#[test]
fn e14_s02_catalog_validation_rejects_unreachable_projects() {
    let bad = [
        vec![project("a", &[], &[]), project("a", &[], &[])],
        vec![project("a", &["missing"], &[])],
        vec![project("a", &[], &["missing"])],
        vec![project("a", &["a"], &[])],
        vec![project("a", &["b"], &[]), project("b", &["a"], &[])],
        vec![project("a", &["b"], &["b"]), project("b", &[], &[])],
        vec![
            project("a", &["b", "c"], &[]),
            project("b", &[], &["c"]),
            project("c", &[], &[]),
        ],
        vec![
            project("a", &["b"], &[]),
            project("b", &["c"], &[]),
            project("c", &[], &["a"]),
        ],
    ];
    for projects in bad {
        assert!(Catalog::new("v1".into(), projects).is_err());
    }
    assert!(Catalog::<u32>::new("v1".into(), vec![]).is_err());
    assert!(Catalog::new(
        "v1".into(),
        (0..=MAX_PROJECTS)
            .map(|i| project(&format!("p{i}"), &[], &[]))
            .collect()
    )
    .is_err());
}
#[test]
fn e14_s02_manifest_is_versioned_deterministic_and_unambiguous() {
    let c = catalog();
    let expected = "carfree-action-catalog\t1\tfixture-v1\nindex\tid\tdescription\trequires\tconflicts\n0\twalk\tSynthetic project walk\t\t\n1\tbus\tSynthetic project bus\twalk\t\n2\tclosure\tSynthetic project closure\t\twalk\n";
    assert_eq!(c.manifest(), expected);
    assert_eq!(c.clone().manifest(), expected);
    for bad in ["", "a\tb", "a\nb", "a,b", "a\u{0000}b"] {
        assert!(Catalog::new("v1".into(), vec![project(bad, &[], &[])]).is_err());
        assert!(Catalog::new(bad.into(), vec![project("a", &[], &[])]).is_err());
    }
    let mut p = project("a", &[], &[]);
    p.description = "column\tbreak".into();
    assert!(Catalog::new("v1".into(), vec![p]).is_err());
}
#[test]
fn e14_s02_diamond_dependencies_are_not_mistaken_for_cycles() {
    let c = Catalog::new(
        "v1".into(),
        vec![
            project("a", &["b", "c"], &[]),
            project("b", &["d"], &[]),
            project("c", &["d"], &[]),
            project("d", &[], &[]),
        ],
    )
    .unwrap();
    assert!(c
        .mask("s", &ids(&["b", "c", "d"]), |_| Ok(vec![]))
        .unwrap()
        .entries[0]
        .allowed());
}
