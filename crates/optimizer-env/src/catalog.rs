//! E14-S02: immutable indexed project catalogs and explanatory feasibility masks.
//!
//! Structural eligibility is deliberately separate from physical screening. A
//! caller must still screen the exact candidate snapshot before committing it.
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u32 = 1;
/// Bound validation work and prerequisite traversal depth at the input boundary.
pub const MAX_PROJECTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

/// Stable IDs and payloads are bound by the immutable catalog. Indices are local
/// to this exact catalog version, never inferred from an arbitrary project ID.
#[derive(Debug, Clone, PartialEq)]
pub struct Project<T> {
    pub id: String,
    pub description: String,
    pub requires: BTreeSet<String>,
    pub conflicts: BTreeSet<String>,
    pub payload: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Catalog<T> {
    revision: String,
    projects: Vec<Project<T>>,
    indices: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    AlreadyApplied,
    MissingPrerequisite(String),
    ConflictingProject(String),
    /// The caller's exact-current-state screening explanation. This must not be
    /// replaced by an assumed feasibility result based on transit presence.
    Screening(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskEntry {
    pub index: usize,
    pub id: String,
    pub reasons: Vec<Reason>,
}
impl MaskEntry {
    pub fn allowed(&self) -> bool {
        self.reasons.is_empty()
    }
}

/// State identity is mandatory to make stale masks detectable. This is not a
/// physical-feasibility certificate and cannot authorize a later state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub version: u32,
    pub catalog_revision: String,
    pub state_revision: String,
    pub entries: Vec<MaskEntry>,
}

fn field(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}
// Catalog IDs are also used in comma-separated references in the audit schema.
fn identifier(value: &str) -> bool {
    field(value) && !value.contains(',')
}
impl<T> Catalog<T> {
    pub fn new(revision: String, projects: Vec<Project<T>>) -> Result<Self, Error> {
        if !identifier(&revision) || projects.is_empty() || projects.len() > MAX_PROJECTS {
            return Err(Error(
                "catalog requires a revision and 1..=256 projects".into(),
            ));
        }
        let mut indices = BTreeMap::new();
        for (index, project) in projects.iter().enumerate() {
            if !identifier(&project.id)
                || !field(&project.description)
                || indices.insert(project.id.clone(), index).is_some()
            {
                return Err(Error("invalid or duplicate project ID/description".into()));
            }
        }
        for project in &projects {
            for reference in project.requires.iter().chain(&project.conflicts) {
                if reference == &project.id || !indices.contains_key(reference) {
                    return Err(Error(format!(
                        "{}: self or unknown reference {reference}",
                        project.id
                    )));
                }
            }
        }
        let catalog = Self {
            revision,
            projects,
            indices,
        };
        // A transitive prerequisite may not conflict with this project or any
        // other required project. Otherwise a catalog advertises unreachable work.
        for project in &catalog.projects {
            let mut visiting = BTreeSet::new();
            let mut required = BTreeSet::new();
            catalog.visit(&project.id, &mut visiting, &mut required)?;
            for a in &required {
                for b in &required {
                    if catalog.conflict(a, b) {
                        return Err(Error(format!(
                            "{}: contradictory prerequisites {a}/{b}",
                            project.id
                        )));
                    }
                }
            }
        }
        Ok(catalog)
    }
    fn visit(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<(), Error> {
        if done.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.into()) {
            return Err(Error(format!("cyclic prerequisites at {id}")));
        }
        for required in &self.projects[self.indices[id]].requires {
            self.visit(required, visiting, done)?;
        }
        visiting.remove(id);
        done.insert(id.into());
        Ok(())
    }
    fn conflict(&self, a: &str, b: &str) -> bool {
        self.projects[self.indices[a]].conflicts.contains(b)
            || self.projects[self.indices[b]].conflicts.contains(a)
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
    pub fn projects(&self) -> &[Project<T>] {
        &self.projects
    }
    pub fn project(&self, index: usize) -> Result<&Project<T>, Error> {
        self.projects
            .get(index)
            .ok_or_else(|| Error(format!("nonexistent action index {index}")))
    }
    fn validate_applied(&self, applied: &BTreeSet<String>) -> Result<(), Error> {
        for id in applied {
            let Some(index) = self.indices.get(id) else {
                return Err(Error(format!("unknown applied project {id}")));
            };
            for required in &self.projects[*index].requires {
                if !applied.contains(required) {
                    return Err(Error(format!(
                        "{id}: applied state lacks prerequisite {required}"
                    )));
                }
            }
            for other in applied {
                if self.conflict(id, other) {
                    return Err(Error(format!(
                        "applied state contains conflicting projects {id}/{other}"
                    )));
                }
            }
        }
        Ok(())
    }
    /// Screening is called only for structurally eligible actions, in catalog
    /// order. Errors abort mask construction, never masquerade as infeasibility.
    pub fn mask(
        &self,
        state_revision: &str,
        applied: &BTreeSet<String>,
        mut screen: impl FnMut(&Project<T>) -> Result<Vec<String>, Error>,
    ) -> Result<Mask, Error> {
        if !field(state_revision) {
            return Err(Error("missing state revision".into()));
        }
        self.validate_applied(applied)?;
        let mut entries = Vec::with_capacity(self.projects.len());
        for (index, p) in self.projects.iter().enumerate() {
            let mut reasons = Vec::new();
            if applied.contains(&p.id) {
                reasons.push(Reason::AlreadyApplied);
            }
            for required in &p.requires {
                if !applied.contains(required) {
                    reasons.push(Reason::MissingPrerequisite(required.clone()));
                }
            }
            for selected in applied {
                if self.conflict(&p.id, selected) {
                    reasons.push(Reason::ConflictingProject(selected.clone()));
                }
            }
            if reasons.is_empty() {
                for reason in screen(p)? {
                    if !field(&reason) {
                        return Err(Error("empty or invalid screening explanation".into()));
                    }
                    reasons.push(Reason::Screening(reason));
                }
            }
            entries.push(MaskEntry {
                index,
                id: p.id.clone(),
                reasons,
            });
        }
        Ok(Mask {
            version: VERSION,
            catalog_revision: self.revision.clone(),
            state_revision: state_revision.into(),
            entries,
        })
    }
    /// Recompute eligibility from current state, rather than accepting a mutable
    /// caller-provided mask as an authorization token. Payload comes from here.
    pub fn resolve(
        &self,
        index: usize,
        expected_catalog: &str,
        expected_state: &str,
        current_state: &str,
        applied: &BTreeSet<String>,
        screen: impl FnMut(&Project<T>) -> Result<Vec<String>, Error>,
    ) -> Result<&Project<T>, Error> {
        self.project(index)?;
        if expected_catalog != self.revision || expected_state != current_state {
            return Err(Error("stale catalog or episode state".into()));
        }
        let mask = self.mask(current_state, applied, screen)?;
        let entry = &mask.entries[index];
        if !entry.allowed() {
            return Err(Error(format!(
                "action {} rejected: {:?}",
                entry.id, entry.reasons
            )));
        }
        self.project(index)
    }
    /// Versioned UTF-8 TSV structural manifest. Payloads remain typed Rust values;
    /// this manifest is not a deserializer or a physical-feasibility certificate.
    pub fn manifest(&self) -> String {
        let mut out = format!(
            "carfree-action-catalog\t{VERSION}\t{}\nindex\tid\tdescription\trequires\tconflicts\n",
            self.revision
        );
        for (index, p) in self.projects.iter().enumerate() {
            let requires = p.requires.iter().cloned().collect::<Vec<_>>().join(",");
            let conflicts = p.conflicts.iter().cloned().collect::<Vec<_>>().join(",");
            out.push_str(&format!(
                "{index}\t{}\t{}\t{requires}\t{conflicts}\n",
                p.id, p.description
            ));
        }
        out
    }
}
