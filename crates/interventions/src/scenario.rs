//! E10 scenario authoring v1: atomic, immutable transport edits and separate land use.
//! All quantities and permissions are caller-supplied modeled assumptions, not
//! engineering approval. See docs/INFRASTRUCTURE-SCENARIOS.md.
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
use mobility_routing::{earliest_arrival, earliest_transit_arrival, Outcome, Request};
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
fn require(ok: bool, message: impl Into<String>) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}

/// One directional physical segment has exactly one car-edge owner. Parallel
/// transit/bicycle representations must never be counted as additional road space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadSpace {
    pub segment: String,
    pub road: EdgeId,
    pub total_lanes: u16,
    pub general_lanes: u16,
    pub bike_lanes: u16,
    pub bus_lanes: u16,
    pub retired_lanes: u16,
    pub vehicles_per_lane_hour: u32,
    pub provenance: Provenance,
}
impl RoadSpace {
    fn capacity(&self) -> Result<u32, Error> {
        self.vehicles_per_lane_hour
            .checked_mul(u32::from(self.general_lanes))
            .ok_or_else(|| Error(format!("capacity overflow on {:?}", self.road)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessKind {
    Emergency,
    Freight,
    Regional,
    Walking,
    Transit,
}
/// Each directed OD/time/profile is an explicit coverage sample, not proof of
/// citywide access. Emergency/freight require an assessed edge permission set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub kind: AccessKind,
    pub request: Request,
    pub permitted_car_edges: Option<BTreeSet<EdgeId>>,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuity {
    pub requirement: String,
    pub kind: AccessKind,
    pub before_seconds: i64,
    pub after_seconds: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// E11 complete directed service replacement, including explicit operations.
    Transit(crate::transit::Service),
    /// E11 station topology and off-road access/parking/transfer facilities.
    Station(crate::transit::Station),
    /// Entire explicit replacement link, including travel time and safety.
    /// Existing targets must retain their mode and endpoints.
    Allocate {
        road: EdgeId,
        lanes: u16,
        target: Link,
        assessment: Option<CrossingAssessment>,
    },
    /// Associate another existing bus service with already allocated physical
    /// bus space. No additional road capacity is consumed.
    AssociateBus {
        road: EdgeId,
        link: Link,
    },
    /// Change a previously allocated bus lane's explicit running time/timetable,
    /// without taking another lane or altering passenger/service identities.
    BusTiming {
        link: Link,
    },
    /// Both crossing and no-crossing classifications require explicit assessment.
    Pedestrian {
        link: Link,
        assessment: CrossingAssessment,
    },
    /// A real crossing's physical time and, for cycling, assessed protection/stress.
    Crossing {
        edge: EdgeId,
        seconds: u32,
        safety: Option<Safety>,
        assessment: CrossingAssessment,
    },
    CloseRoad {
        edge: EdgeId,
    },
    LandUse {
        destinations: Vec<Destination>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossingAssessment {
    pub crossing: bool,
    pub permitted: bool,
    pub minimum_seconds: u32,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub id: String,
    pub node: NodeId,
    pub category: String,
    pub opportunities: u32,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioKind {
    TransportOnly,
    TransportPlusLandUse,
}
/// A validated, immutable version. Even a cloned snapshot cannot be mutated.
/// ```compile_fail
/// use interventions::scenario::Snapshot;
/// fn tamper(mut snapshot: Snapshot) { snapshot.id = "forged".into(); }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    data: SnapshotData,
}
/// Read-only snapshot view. A view may be cloned, but cannot be turned into a
/// validated Snapshot. Only History constructs snapshots; there is no DerefMut.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotData {
    pub version: u32,
    pub id: String,
    pub parent: Option<String>,
    pub network: Network,
    pub roads: BTreeMap<EdgeId, RoadSpace>,
    pub allocations: BTreeMap<EdgeId, EdgeId>,
    pub crossings: BTreeMap<EdgeId, CrossingAssessment>,
    pub destinations: Vec<Destination>,
    pub land_use_revision: Option<String>,
    pub provenance: Provenance,
    pub continuity: Vec<Continuity>,
    /// E11 modeled operations, recomputed for each committed transport version.
    pub transit: BTreeMap<String, crate::transit::Record>,
}
impl std::ops::Deref for Snapshot {
    type Target = SnapshotData;
    fn deref(&self) -> &SnapshotData {
        &self.data
    }
}
impl Snapshot {
    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn id(&self) -> &String {
        &self.id
    }
    pub fn parent(&self) -> &Option<String> {
        &self.parent
    }
    pub fn network(&self) -> &Network {
        &self.network
    }
    pub fn roads(&self) -> &BTreeMap<EdgeId, RoadSpace> {
        &self.roads
    }
    pub fn allocations(&self) -> &BTreeMap<EdgeId, EdgeId> {
        &self.allocations
    }
    pub fn crossings(&self) -> &BTreeMap<EdgeId, CrossingAssessment> {
        &self.crossings
    }
    pub fn destinations(&self) -> &[Destination] {
        &self.destinations
    }
    pub fn land_use_revision(&self) -> &Option<String> {
        &self.land_use_revision
    }
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
    pub fn continuity(&self) -> &[Continuity] {
        &self.continuity
    }
    pub fn kind(&self) -> ScenarioKind {
        if self.destinations.is_empty() {
            ScenarioKind::TransportOnly
        } else {
            ScenarioKind::TransportPlusLandUse
        }
    }
    /// Single authoritative constant road supply, vehicles/hour. Caller explicitly
    /// maps this to E08 periods; never add baseline supply or duplicate edits.
    /// Missing capacities fail rather than inventing values.
    pub fn road_capacities(&self) -> Result<Vec<(EdgeId, u32)>, Error> {
        self.network
            .data()
            .links
            .iter()
            .filter_map(|l| match &l.kind {
                LinkKind::Street {
                    mode: Mode::Car,
                    blocked: false,
                    capacity,
                    ..
                } => Some(
                    capacity
                        .as_ref()
                        .map(|c| (l.id, c.vehicles_per_hour))
                        .ok_or_else(|| Error(format!("missing road capacity {:?}", l.id))),
                ),
                _ => None,
            })
            .collect()
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub expected_parent: String,
    /// Unique scenario-version identity; also graph revision for transport edits.
    pub id: String,
    pub provenance: Provenance,
    pub changes: Vec<Change>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    snapshots: Vec<Snapshot>,
    cursor: usize,
    used_ids: BTreeSet<String>,
    requirements: Vec<Requirement>,
}
pub(crate) fn validate_provenance(n: &Network, p: &Provenance, modeled: bool) -> Result<(), Error> {
    require(
        !modeled || p.basis == Basis::Modeled,
        "interventions must be labeled modeled",
    )?;
    let mut d = n.data().clone();
    d.provenance = p.clone();
    Network::new(d).map_err(|e| Error(e.to_string()))?;
    Ok(())
}
fn street_mode(l: &Link) -> Option<Mode> {
    match l.kind {
        LinkKind::Street { mode, .. } => Some(mode),
        _ => None,
    }
}
fn usable(l: &Link) -> bool {
    !matches!(l.access, Access::Impassable | Access::Unknown)
        && !matches!(l.kind, LinkKind::Street { blocked: true, .. })
}
fn mode(l: &Link) -> Option<Mode> {
    match l.kind {
        LinkKind::Street { mode, .. } | LinkKind::Transit { mode, .. } => Some(mode),
        _ => None,
    }
}
fn road_mut(d: &mut NetworkData, id: EdgeId) -> Result<&mut Link, Error> {
    d.links
        .iter_mut()
        .find(|l| l.id == id)
        .ok_or_else(|| Error(format!("unknown edge {id:?}")))
}
fn set_capacity(d: &mut NetworkData, r: &RoadSpace, p: &Provenance) -> Result<(), Error> {
    let capacity = r.capacity()?;
    let l = road_mut(d, r.road)?;
    if let LinkKind::Street {
        mode: Mode::Car,
        blocked,
        capacity: c,
        ..
    } = &mut l.kind
    {
        *blocked = capacity == 0;
        *c = (capacity > 0).then(|| {
            Box::new(Capacity {
                vehicles_per_hour: capacity,
                provenance: p.clone(),
            })
        });
        Ok(())
    } else {
        Err(Error(format!("not a car road {:?}", r.road)))
    }
}
fn validate_replacement(old: &Link, new: &Link) -> Result<(), Error> {
    require(
        old.from == new.from && old.to == new.to && mode(old) == mode(new),
        "replacement changes target topology/mode",
    )?;
    if let (
        LinkKind::Transit {
            trip: a,
            stop_index: ai,
            calendar: ac,
            timetable: at,
            ..
        },
        LinkKind::Transit {
            trip: b,
            stop_index: bi,
            calendar: bc,
            timetable: bt,
            ..
        },
    ) = (&old.kind, &new.kind)
    {
        let runs = |t: &Timetable, sec| -> Result<BTreeSet<u32>, Error> {
            Ok(t.departures(sec)
                .map_err(|e| Error(e.to_string()))?
                .map(|d| d.run)
                .collect())
        };
        require(
            a == b && ai == bi && ac == bc && runs(at, old.seconds)? == runs(bt, new.seconds)?,
            "bus edit must preserve service/run identity; supply changes are separate",
        )?;
        let mut timing_only = old.clone();
        timing_only.seconds = new.seconds;
        if let LinkKind::Transit { timetable, .. } = &mut timing_only.kind {
            *timetable = bt.clone();
        }
        require(
            timing_only == *new,
            "bus timing must preserve access, carriage and all non-timing fields",
        )?;
    }
    if matches!(old.kind, LinkKind::Street { crossing: true, .. }) {
        require(
            matches!(new.kind, LinkKind::Street { crossing: true, .. }),
            "replacement cannot erase baseline crossing classification",
        )?;
    }
    Ok(())
}
pub(crate) fn validate_assessment(
    n: &Network,
    link: &Link,
    assessment: &CrossingAssessment,
    recorded: Option<&CrossingAssessment>,
) -> Result<(), Error> {
    if let Some(prior) = recorded {
        require(
            prior.permitted
                && assessment.minimum_seconds >= prior.minimum_seconds
                && assessment.crossing == prior.crossing,
            "crossing edit weakens recorded minimum/classification/permission",
        )?;
    }
    validate_provenance(n, &assessment.provenance, true)?;
    require(
        assessment.permitted
            && assessment.minimum_seconds > 0
            && link.seconds >= assessment.minimum_seconds
            && matches!(link.kind, LinkKind::Street { crossing, .. } if crossing == assessment.crossing),
        "crossing classification/permission/minimum time mismatch",
    )
}
fn duration(n: &Network, r: &Requirement) -> Result<i64, Error> {
    let mut filtered;
    let network = if let Some(permissions) = &r.permitted_car_edges {
        filtered = n.data().clone();
        for l in &mut filtered.links {
            if let LinkKind::Street {
                mode: Mode::Car,
                blocked,
                ..
            } = &mut l.kind
            {
                *blocked |= !permissions.contains(&l.id);
            }
        }
        filtered.revision = n.data().revision.clone();
        // Keep turn rules even when their only permitted destination is blocked.
        // Deleting them could accidentally legalize an otherwise forbidden detour.
        Network::new(filtered).map_err(|e| Error(e.to_string()))?
    } else {
        n.clone()
    };
    let route = if r.kind == AccessKind::Transit {
        earliest_transit_arrival(&network, &r.request)
    } else {
        earliest_arrival(&network, &r.request)
    };
    match route.map_err(|e| Error(format!("requirement {}: {e}", r.id)))? {
        Outcome::Journey(j) => Ok(j.arrival_utc - j.departure_utc),
        Outcome::Unmet(_) => Err(Error(format!("unmet {:?} requirement {}", r.kind, r.id))),
    }
}
impl History {
    pub fn new(
        id: String,
        network: Network,
        roads: Vec<RoadSpace>,
        requirements: Vec<Requirement>,
    ) -> Result<Self, Error> {
        require(!id.trim().is_empty(), "empty scenario id")?;
        let mut ledger = BTreeMap::new();
        let mut segments = BTreeSet::new();
        for r in roads {
            validate_provenance(&network, &r.provenance, false)?;
            require(
                !r.segment.trim().is_empty() && segments.insert(r.segment.clone()),
                "duplicate/empty directional segment owner",
            )?;
            let sum = u32::from(r.general_lanes)
                + u32::from(r.bike_lanes)
                + u32::from(r.bus_lanes)
                + u32::from(r.retired_lanes);
            require(
                r.total_lanes > 0
                    && sum == u32::from(r.total_lanes)
                    && r.vehicles_per_lane_hour > 0,
                format!("invalid lane inventory {:?}", r.road),
            )?;
            let cap = r.capacity()?;
            require(network.link(r.road).is_some_and(|l| matches!(&l.kind,
                LinkKind::Street { mode: Mode::Car, blocked, capacity, .. }
                    if *blocked == (cap == 0) && capacity.as_ref().map(|c| c.vehicles_per_hour) == if cap == 0 { None } else { Some(cap) })),
                format!("lane inventory disagrees with road {:?}", r.road))?;
            require(ledger.insert(r.road, r).is_none(), "duplicate road owner")?;
        }
        let mut ids = BTreeSet::new();
        for r in &requirements {
            validate_provenance(&network, &r.provenance, false)?;
            require(
                !r.id.trim().is_empty()
                    && ids.insert(r.id.clone())
                    && r.request.from != r.request.to,
                "duplicate/empty/vacuous requirement",
            )?;
            match r.kind {
                AccessKind::Emergency | AccessKind::Freight | AccessKind::Regional => {
                    require(
                        r.request.initial_mode == Mode::Car
                            && r.request.profile.allowed_modes == vec![Mode::Car],
                        "vehicle requirement must use car-only routing",
                    )?;
                }
                AccessKind::Walking => require(
                    r.request.profile.allowed_modes == vec![Mode::Walk],
                    "walking requirement must use walk-only routing",
                )?,
                AccessKind::Transit => require(
                    !r.request.profile.allowed_modes.contains(&Mode::Car)
                        && !r
                            .request
                            .profile
                            .allowed_modes
                            .iter()
                            .any(|m| is_bicycle(*m)),
                    "transit requirement must not substitute car/cycling",
                )?,
            }
            if matches!(r.kind, AccessKind::Emergency | AccessKind::Freight) {
                require(
                    r.permitted_car_edges
                        .as_ref()
                        .is_some_and(|p| !p.is_empty()),
                    "missing emergency/freight edge permissions",
                )?;
            }
            if let Some(p) = &r.permitted_car_edges {
                require(
                    p.iter().all(|id| {
                        network
                            .link(*id)
                            .is_some_and(|l| street_mode(l) == Some(Mode::Car))
                    }),
                    "unknown/non-car permission edge",
                )?;
            }
            duration(&network, r)?;
        }
        let used_ids = BTreeSet::from([id.clone(), network.data().revision.clone()]);
        let snapshot = Snapshot {
            data: SnapshotData {
                version: VERSION,
                id: id.clone(),
                parent: None,
                provenance: network.data().provenance.clone(),
                network,
                roads: ledger,
                allocations: BTreeMap::new(),
                crossings: BTreeMap::new(),
                destinations: vec![],
                land_use_revision: None,
                continuity: vec![],
                transit: BTreeMap::new(),
            },
        };
        Ok(Self {
            snapshots: vec![snapshot],
            cursor: 0,
            used_ids,
            requirements,
        })
    }
    pub fn current(&self) -> &Snapshot {
        &self.snapshots[self.cursor]
    }
    pub fn undo(&mut self) -> bool {
        if self.cursor == 0 {
            false
        } else {
            self.cursor -= 1;
            true
        }
    }
    pub fn redo(&mut self) -> bool {
        if self.cursor + 1 == self.snapshots.len() {
            false
        } else {
            self.cursor += 1;
            true
        }
    }
    /// Validate without mutation, using exactly the same path as apply.
    pub fn preview(&self, batch: &Batch) -> Result<Snapshot, Error> {
        let parent = self.current();
        require(batch.expected_parent == parent.id, "stale scenario parent")?;
        require(
            !batch.id.trim().is_empty() && !self.used_ids.contains(&batch.id),
            "empty/reused scenario id",
        )?;
        require(!batch.changes.is_empty(), "empty edit batch")?;
        validate_provenance(&parent.network, &batch.provenance, true)?;
        crate::transit::validate_batch(&parent.network, &batch.changes)?;
        let mut s = parent.data.clone();
        let mut d = s.network.data().clone();
        let mut transport = false;
        let mut closes = false;
        let mut targets = BTreeSet::new();
        let mut land_use_changed = false;
        let mut service_targets = BTreeSet::new();
        for c in &batch.changes {
            match c {
                Change::Transit(service) => {
                    require(
                        service_targets.insert(service.trip.clone()),
                        "duplicate service replacement in batch",
                    )?;
                    crate::transit::service(&mut s, &mut d, service, &batch.provenance)?;
                    transport = true;
                }
                Change::Station(station) => {
                    crate::transit::station(&mut s, &mut d, station)?;
                    transport = true;
                }
                Change::Allocate {
                    road,
                    lanes,
                    target,
                    assessment,
                } => {
                    require(
                        *lanes > 0 && targets.insert(target.id),
                        "zero lanes or duplicate target in batch",
                    )?;
                    let source = road_mut(&mut d, *road)?.clone();
                    require(
                        usable(&source)
                            && target.from == source.from
                            && target.to == source.to
                            && usable(target),
                        "allocation requires usable matching directed endpoints",
                    )?;
                    let cycling = matches!(
                        street_mode(target),
                        Some(Mode::Bicycle | Mode::ElectricBicycle)
                    );
                    require(
                        cycling
                            || matches!(
                                target.kind,
                                LinkKind::Transit {
                                    mode: Mode::Bus,
                                    ..
                                }
                            ),
                        "allocation target must be bicycle or bus",
                    )?;
                    if cycling {
                        require(
                            matches!(&target.kind, LinkKind::Street { cycling: CycleFacility::Protected, safety: Some(safety), .. } if safety.provenance.basis == Basis::Modeled),
                            "protected bike allocation requires modeled safety assessment",
                        )?;
                        let approval = assessment.as_ref().ok_or_else(|| {
                            Error("cycling allocation needs crossing assessment".into())
                        })?;
                        validate_assessment(
                            &parent.network,
                            target,
                            approval,
                            s.crossings.get(&target.id),
                        )?;
                        s.crossings.insert(target.id, approval.clone());
                    } else {
                        require(
                            assessment.is_none(),
                            "bus timing is not a street crossing assessment",
                        )?;
                        require(
                            d.links.iter().any(|l| l.id == target.id),
                            "bus allocation must reference existing service",
                        )?;
                    }
                    if let Some(old) = d.links.iter().find(|l| l.id == target.id) {
                        validate_replacement(old, target)?;
                        require(
                            old.access != Access::Accessible || target.access == Access::Accessible,
                            "allocation cannot downgrade accessible infrastructure",
                        )?;
                    }
                    require(
                        s.allocations
                            .get(&target.id)
                            .is_none_or(|owner| owner == road),
                        "target already owned by another physical segment",
                    )?;
                    s.allocations.insert(target.id, *road);
                    let r = s
                        .roads
                        .get_mut(road)
                        .ok_or_else(|| Error(format!("missing lane inventory {road:?}")))?;
                    require(r.general_lanes >= *lanes, "lane over-allocation")?;
                    r.general_lanes -= lanes;
                    if cycling {
                        r.bike_lanes += lanes;
                    } else {
                        r.bus_lanes += lanes;
                    }
                    r.provenance = batch.provenance.clone();
                    set_capacity(&mut d, r, &batch.provenance)?;
                    closes |= r.general_lanes == 0;
                    if let Some(old) = d.links.iter_mut().find(|l| l.id == target.id) {
                        *old = target.clone();
                    } else {
                        d.links.push(target.clone());
                    }
                    transport = true;
                }
                Change::AssociateBus { road, link } => {
                    let source = road_mut(&mut d, *road)?.clone();
                    require(s.roads.get(road).is_some_and(|r| r.bus_lanes > 0)
                        && matches!(link.kind, LinkKind::Transit { mode: Mode::Bus, .. })
                        && link.from == source.from && link.to == source.to && usable(link)
                        && targets.insert(link.id) && s.allocations.get(&link.id).is_none_or(|owner| owner == road),
                        "bus association requires existing physical bus space and matching endpoints")?;
                    let old = road_mut(&mut d, link.id)?;
                    validate_replacement(old, link)?;
                    *old = link.clone();
                    s.allocations.insert(link.id, *road);
                    transport = true;
                }
                Change::BusTiming { link } => {
                    require(
                        matches!(
                            link.kind,
                            LinkKind::Transit {
                                mode: Mode::Bus,
                                ..
                            }
                        ) && usable(link)
                            && s.allocations.contains_key(&link.id)
                            && targets.insert(link.id),
                        "bus timing requires a unique allocated bus lane",
                    )?;
                    let old = road_mut(&mut d, link.id)?;
                    validate_replacement(old, link)?;
                    *old = link.clone();
                    transport = true;
                }
                Change::Pedestrian { link, assessment } => {
                    validate_assessment(
                        &parent.network,
                        link,
                        assessment,
                        s.crossings.get(&link.id),
                    )?;
                    require(
                        street_mode(link) == Some(Mode::Walk) && usable(link),
                        "pedestrian link must be usable walking infrastructure",
                    )?;
                    require(targets.insert(link.id), "duplicate target in batch")?;
                    for node in [link.from, link.to] {
                        require(
                            parent.network.node(node).is_some_and(|n| {
                                matches!(n.access, Access::Accessible | Access::Steps)
                            }),
                            "inaccessible pedestrian endpoint",
                        )?;
                        require(
                            parent.network.data().links.iter().any(|l| {
                                l.id != link.id
                                    && street_mode(l) == Some(Mode::Walk)
                                    && usable(l)
                                    && (if node == link.from {
                                        l.to == node
                                    } else {
                                        l.from == node
                                    })
                            }),
                            "pedestrian endpoint not attached to existing walking graph",
                        )?;
                    }
                    if let Some(old) = d.links.iter_mut().find(|l| l.id == link.id) {
                        validate_replacement(old, link)?;
                        require(
                            street_mode(old) == Some(Mode::Walk)
                                && old.from == link.from
                                && old.to == link.to,
                            "pedestrian replacement changes topology/mode",
                        )?;
                        require(
                            old.access != Access::Accessible || link.access == Access::Accessible,
                            "pedestrian improvement cannot downgrade accessible infrastructure",
                        )?;
                        *old = link.clone();
                    } else {
                        d.links.push(link.clone());
                    }
                    s.crossings.insert(link.id, assessment.clone());
                    transport = true;
                }
                Change::Crossing {
                    edge,
                    seconds,
                    safety,
                    assessment,
                } => {
                    require(targets.insert(*edge), "duplicate target in batch")?;
                    let l = road_mut(&mut d, *edge)?;
                    require(usable(l), "blocked/inaccessible crossing")?;
                    if let LinkKind::Street {
                        mode,
                        crossing: true,
                        cycling,
                        safety: old,
                        ..
                    } = &mut l.kind
                    {
                        require(
                            *mode == Mode::Walk || is_bicycle(*mode),
                            "crossing must be walking/cycling",
                        )?;
                        if is_bicycle(*mode) {
                            let s = safety.as_ref().ok_or_else(|| {
                                Error("cycling crossing needs safety assessment".into())
                            })?;
                            validate_provenance(&parent.network, &s.provenance, true)?;
                            *cycling = CycleFacility::Protected;
                            *old = Some(Box::new(s.clone()));
                        } else {
                            require(safety.is_none(), "walking crossing has no cycling safety")?;
                        }
                        l.seconds = *seconds;
                    } else {
                        return Err(Error(format!("not a crossing {edge:?}")));
                    }
                    validate_assessment(&parent.network, l, assessment, s.crossings.get(edge))?;
                    s.crossings.insert(*edge, assessment.clone());
                    transport = true;
                }
                Change::CloseRoad { edge } => {
                    require(targets.insert(*edge), "duplicate target in batch")?;
                    let r = s
                        .roads
                        .get_mut(edge)
                        .ok_or_else(|| Error(format!("missing lane inventory {edge:?}")))?;
                    require(r.general_lanes > 0, "road already closed")?;
                    r.retired_lanes += r.general_lanes;
                    r.general_lanes = 0;
                    r.provenance = batch.provenance.clone();
                    set_capacity(&mut d, r, &batch.provenance)?;
                    closes = true;
                    transport = true;
                }
                Change::LandUse { destinations } => {
                    require(
                        !land_use_changed && !destinations.is_empty(),
                        "empty/duplicate land-use extension",
                    )?;
                    land_use_changed = true;
                    let mut ids = BTreeSet::new();
                    for dest in destinations {
                        require(
                            !dest.id.trim().is_empty()
                                && ids.insert(dest.id.clone())
                                && !dest.category.trim().is_empty()
                                && parent.network.node(dest.node).is_some(),
                            "invalid/duplicate destination",
                        )?;
                        validate_provenance(&parent.network, &dest.provenance, true)?;
                    }
                    s.destinations = destinations.clone();
                    s.land_use_revision = Some(batch.id.clone());
                }
            }
        }
        if closes {
            for kind in [
                AccessKind::Emergency,
                AccessKind::Freight,
                AccessKind::Regional,
                AccessKind::Walking,
                AccessKind::Transit,
            ] {
                require(
                    self.requirements.iter().any(|r| r.kind == kind),
                    format!("closure requires explicit {kind:?} coverage"),
                )?;
            }
        }
        if transport {
            require(
                batch.id != parent.network.data().revision,
                "transport edit needs a new graph revision",
            )?;
            d.revision = batch.id.clone();
            d.provenance = batch.provenance.clone();
            s.network = Network::new(d).map_err(|e| Error(e.to_string()))?;
            for (trip, record) in &mut s.transit {
                record.estimate = crate::transit::estimate(&s.network, trip, &record.operations)?;
            }
            if !s.transit.is_empty()
                || batch
                    .changes
                    .iter()
                    .any(|c| matches!(c, Change::Station(_)))
            {
                crate::transit::validate_facilities(&parent.network, &s.network, &s.transit)?;
                crate::transit::validate_station_opportunities(
                    &parent.network,
                    &s.network,
                    &batch.changes,
                )?;
            }
        }
        s.continuity = self
            .requirements
            .iter()
            .map(|r| {
                Ok(Continuity {
                    requirement: r.id.clone(),
                    kind: r.kind,
                    before_seconds: duration(&parent.network, r)?,
                    after_seconds: duration(&s.network, r)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        s.id = batch.id.clone();
        s.parent = Some(parent.id.clone());
        s.provenance = batch.provenance.clone();
        Ok(Snapshot { data: s })
    }
    pub fn apply(&mut self, batch: &Batch) -> Result<&Snapshot, Error> {
        let s = self.preview(batch)?;
        self.snapshots.truncate(self.cursor + 1);
        self.used_ids.insert(s.id.clone());
        self.snapshots.push(s);
        self.cursor += 1;
        Ok(self.current())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorridorPreview {
    pub edges: Vec<EdgeId>,
    pub issues: Vec<String>,
}
/// Ordered directed selection: gaps, unusable links, mixed modes and car turn
/// restrictions are visible before a batch is authored. No geometry inference.
pub fn preview_corridor(
    network: &Network,
    edges: &[EdgeId],
    intended_mode: Mode,
) -> CorridorPreview {
    let mut issues = vec![];
    if edges.is_empty() {
        issues.push("empty corridor".into());
    }
    let mut seen = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    if let Some(first) = edges.first().and_then(|id| network.link(*id)) {
        nodes.insert(first.from);
    }
    for id in edges {
        match network.link(*id) {
            Some(l) => {
                if !nodes.insert(l.to) {
                    issues.push(format!(
                        "repeated corridor node {:?}: only simple paths supported",
                        l.to
                    ));
                }
                if !seen.insert(*id) {
                    issues.push(format!("duplicate edge {id:?}"));
                }
                if mode(l) != Some(intended_mode) || !usable(l) {
                    issues.push(format!("incompatible edge {id:?}"));
                }
            }
            None => issues.push(format!("missing edge {id:?}")),
        }
    }
    for pair in edges.windows(2) {
        if let (Some(a), Some(b)) = (network.link(pair[0]), network.link(pair[1])) {
            if a.to != b.from {
                issues.push(format!("gap {:?} -> {:?}", a.id, b.id));
            }
            if network
                .node(a.to)
                .is_none_or(|n| !matches!(n.access, Access::Accessible | Access::Steps))
            {
                issues.push(format!("inaccessible junction {:?}", a.to));
            }
            if intended_mode == Mode::Car
                && network.data().turns.iter().any(|t| {
                    t.incoming == a.id
                        && ((t.only && t.outgoing != b.id) || (!t.only && t.outgoing == b.id))
                })
            {
                issues.push(format!("forbidden turn {:?} -> {:?}", a.id, b.id));
            }
        }
    }
    CorridorPreview {
        edges: edges.to_vec(),
        issues,
    }
}
/// Select a legal connected route with the caller's actual mobility profile.
/// Transfer edges are retained; route feasibility, not endpoint adjacency, wins.
pub fn select_corridor(network: &Network, request: &Request) -> Result<Vec<EdgeId>, Error> {
    require(request.from != request.to, "corridor endpoints must differ")?;
    match earliest_arrival(network, request).map_err(|e| Error(e.to_string()))? {
        Outcome::Journey(j) => Ok(j.steps.into_iter().map(|s| s.edge).collect()),
        Outcome::Unmet(_) => Err(Error(
            "no feasible corridor for supplied profile/time".into(),
        )),
    }
}
