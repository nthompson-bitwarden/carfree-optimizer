use super::*;

type Tags = BTreeMap<String, String>;
type Position = [f64; 2];
type CoordinateKey = [u64; 2];
type Fixes<'a> = BTreeMap<(&'a str, &'a str, &'a str), (usize, &'a StopFix)>;
const EARTH_METERS: f64 = 6_371_008.8;

#[derive(Deserialize)]
struct Osm {
    nodes: BTreeMap<i64, Node>,
    ways: BTreeMap<i64, Way>,
}

#[derive(Deserialize, Serialize)]
struct Node {
    coordinates: Position,
    tags: Tags,
}

#[derive(Deserialize)]
struct Way {
    tags: Tags,
    segments: Vec<Segment>,
}

#[derive(Deserialize)]
struct Segment {
    index: usize,
    from: Reference,
    to: Reference,
}

#[derive(Deserialize)]
struct Reference {
    id: i64,
}

#[derive(Serialize)]
struct Edge {
    way: i64,
    segment: usize,
    from: i64,
    to: i64,
    length_m: f64,
    walk_candidate: bool,
}

#[derive(Serialize)]
pub(super) struct Network {
    nodes: BTreeMap<i64, Node>,
    aliases: BTreeMap<i64, i64>,
    edges: Vec<Edge>,
    components: BTreeMap<i64, i64>,
    walk_components: BTreeMap<i64, i64>,
    #[serde(skip)]
    stop_nodes: BTreeMap<CoordinateKey, i64>,
}

impl Network {
    pub(super) fn is_walk_node(&self, node: i64) -> bool {
        self.walk_components.contains_key(&node)
    }
}

fn tag<'a>(tags: &'a Tags, key: &str) -> &'a str {
    tags.get(key).map(String::as_str).unwrap_or("")
}

fn key([lon, lat]: Position) -> CoordinateKey {
    let lon = if lat.abs() == 90.0 {
        0.0
    } else if lon == 180.0 {
        -180.0
    } else {
        lon
    };
    let bits = |value: f64| if value == 0.0 { 0 } else { value.to_bits() };
    [bits(lon), bits(lat)]
}

fn distance([x, y]: Position, [u, v]: Position) -> f64 {
    let a = ((v - y).to_radians() * 0.5).sin().powi(2)
        + y.to_radians().cos() * v.to_radians().cos() * ((u - x).to_radians() * 0.5).sin().powi(2);
    2.0 * EARTH_METERS * a.clamp(0.0, 1.0).sqrt().asin()
}

fn at_grade(tags: &Tags) -> bool {
    matches!(tag(tags, "layer"), "" | "0")
        && ["bridge", "tunnel", "covered", "indoor"]
            .into_iter()
            .all(|k| matches!(tag(tags, k), "" | "no"))
        && tag(tags, "level").is_empty()
}

fn walk(tags: &Tags) -> bool {
    // Conservative screening: no inferred pedestrian permissions on general roads.
    at_grade(tags)
        && tag(tags, "barrier").is_empty()
        && ["access", "foot"]
            .into_iter()
            .all(|k| matches!(tag(tags, k), "" | "yes" | "designated" | "permissive"))
        && !tags.keys().any(|k| {
            k.starts_with("access:") || k.starts_with("foot:") || k.starts_with("oneway:foot")
        })
        && matches!(tag(tags, "oneway"), "" | "no")
}

fn walk_way(tags: &Tags) -> bool {
    walk(tags)
        && matches!(tag(tags, "area"), "" | "no")
        && (matches!(
            tag(tags, "highway"),
            "footway" | "pedestrian" | "living_street"
        ) || (matches!(tag(tags, "foot"), "yes" | "designated")
            && !matches!(
                tag(tags, "highway"),
                "motorway" | "motorway_link" | "construction" | "proposed"
            )))
}

fn components(edges: &[Edge], walking: bool) -> BTreeMap<i64, i64> {
    let mut adjacency = BTreeMap::<_, Vec<_>>::new();
    for e in edges.iter().filter(|e| !walking || e.walk_candidate) {
        adjacency.entry(e.from).or_default().push(e.to);
        adjacency.entry(e.to).or_default().push(e.from);
    }
    let mut component = BTreeMap::new();
    let mut queue = Vec::new();
    for &node in adjacency.keys() {
        if component.contains_key(&node) {
            continue;
        }
        component.insert(node, node);
        queue.push(node);
        while let Some(next) = queue.pop() {
            for &neighbor in &adjacency[&next] {
                if let std::collections::btree_map::Entry::Vacant(entry) = component.entry(neighbor)
                {
                    entry.insert(node);
                    queue.push(neighbor);
                }
            }
        }
    }
    component
}

pub(super) fn network(data: &Value, findings: &mut Vec<Finding>) -> Result<Network> {
    let Osm { nodes, ways } =
        Osm::deserialize(data).map_err(|e| Error::new(format!("OSM connector contract: {e}")))?;
    let mut coincident = BTreeMap::<_, Vec<_>>::new();
    let mut incident = BTreeMap::<_, Vec<_>>::new();
    for (&id, node) in &nodes {
        coincident
            .entry(key(node.coordinates))
            .or_default()
            .push(id);
    }
    for (&id, way) in &ways {
        for segment in &way.segments {
            incident.entry(segment.from.id).or_default().push(id);
            incident.entry(segment.to.id).or_default().push(id);
        }
    }
    let mut aliases = BTreeMap::new();
    // Only exact, untagged endpoint duplicates of the same named/ref-tagged surface corridor.
    // No proximity chaining, no joining geometric crossings, no restricted-way rewrites.
    if data["restrictions"]
        .as_object()
        .is_some_and(|r| r.is_empty())
    {
        for ids in coincident.values() {
            if let [a, b] = ids.as_slice() {
                if let ([first], [second]) = (incident[a].as_slice(), incident[b].as_slice()) {
                    let tags = &ways[first].tags;
                    if first != second
                        && tags == &ways[second].tags
                        && at_grade(tags)
                        && walk(tags)
                        && (!tag(tags, "name").is_empty() || !tag(tags, "ref").is_empty())
                        && nodes[a].tags.is_empty()
                        && nodes[b].tags.is_empty()
                    {
                        aliases.insert(*b, *a);
                        findings.push(finding("snapped_endpoint", b, format!("coincident corridor endpoint aliased to {a}; inferred, review before routing"), false));
                    }
                }
            }
        }
    }
    let snapped: BTreeSet<_> = aliases.values().copied().collect();
    let mut edges = Vec::new();
    let mut seen = BTreeMap::new();
    let mut degrees = BTreeMap::<_, usize>::new();
    for (&way_id, way) in &ways {
        attributes(way_id, &way.tags, findings);
        for s in &way.segments {
            let from = aliases.get(&s.from.id).copied().unwrap_or(s.from.id);
            let to = aliases.get(&s.to.id).copied().unwrap_or(s.to.id);
            let id = format!("way {way_id} segment {}", s.index);
            let start = nodes[&from].coordinates;
            let end = nodes[&to].coordinates;
            if key(start) == key(end) || (start[1] == -end[1] && (start[0] - end[0]).abs() == 180.0)
            {
                findings.push(finding("impossible_edge", id, "coincident endpoints or ambiguous antipodal segment; excluded from QA topology", true));
                continue;
            }
            let (mut a, mut b) = (key(start), key(end));
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            if let Some(other) = seen.get(&(a, b, grade(&way.tags))) {
                // Retain source edges: these may represent distinct travel permissions.
                findings.push(finding(
                    "duplicate_edge",
                    &id,
                    format!("same-grade geometry also used by {other}; source edges retained"),
                    false,
                ));
            } else {
                seen.insert((a, b, grade(&way.tags)), id);
            }
            *degrees.entry(from).or_default() += 1;
            *degrees.entry(to).or_default() += 1;
            let walk_candidate = walk_way(&way.tags)
                && walk(&nodes[&from].tags)
                && walk(&nodes[&to].tags)
                && !aliases.contains_key(&s.from.id)
                && !aliases.contains_key(&s.to.id)
                && !snapped.contains(&from)
                && !snapped.contains(&to);
            edges.push(Edge {
                way: way_id,
                segment: s.index,
                from,
                to,
                length_m: distance(start, end),
                walk_candidate,
            });
        }
    }
    for (node, degree) in degrees {
        if degree == 1 {
            findings.push(finding(
                "dangling_link",
                node,
                "degree-one endpoint; verify a real dead end or extract boundary",
                false,
            ));
        }
    }
    let mut grade_conflicts = BTreeSet::new();
    for (&node, ids) in &incident {
        let mut counts = BTreeMap::<_, usize>::new();
        for &way in ids {
            *counts.entry(way).or_default() += 1;
        }
        let grades: BTreeSet<_> = counts
            .iter()
            .filter(|(_, n)| **n > 1)
            .map(|(way, _)| grade(&ways[way].tags))
            .collect();
        if grades.len() > 1 {
            findings.push(finding("grade_conflict", node, "different-grade through ways reuse a node; do not route through this junction without review", true));
            grade_conflicts.insert(node);
        }
    }
    for edge in &mut edges {
        edge.walk_candidate &=
            !grade_conflicts.contains(&edge.from) && !grade_conflicts.contains(&edge.to);
    }
    let all_components = components(&edges, false);
    let walk_components = components(&edges, true);
    let distinct: BTreeSet<_> = all_components.values().copied().collect();
    if distinct.len() > 1 {
        for id in distinct {
            findings.push(finding(
                "disconnected_component",
                id,
                "separate weak network component, check extract and access",
                false,
            ));
        }
    }
    let mut stop_nodes = BTreeMap::new();
    for (coordinate, ids) in coincident {
        if let [node] = ids.as_slice() {
            if walk_components.contains_key(node) {
                stop_nodes.insert(coordinate, *node);
            }
        }
    }
    Ok(Network {
        nodes,
        aliases,
        edges,
        components: all_components,
        walk_components,
        stop_nodes,
    })
}

fn grade(tags: &Tags) -> (&str, bool, bool, &str) {
    let layer = match tag(tags, "layer") {
        "0" => "",
        value => value,
    };
    (
        layer,
        !matches!(tag(tags, "bridge"), "" | "no"),
        !matches!(tag(tags, "tunnel"), "" | "no"),
        tag(tags, "level"),
    )
}

fn attributes(way: i64, tags: &Tags, findings: &mut Vec<Finding>) {
    if let Some(value) = tags.get("lanes") {
        match value.parse::<u32>() {
            Ok(1..=12) => {}
            _ => findings.push(finding(
                "lanes_review",
                way,
                format!("lanes={value:?}; expected integer 1–12"),
                true,
            )),
        }
    }
    if let Some(value) = tags.get("maxspeed") {
        let (number, multiplier) = value
            .strip_suffix(" mph")
            .map(|n| (n, 0.44704))
            .unwrap_or((value.strip_suffix(" km/h").unwrap_or(value), 1.0 / 3.6));
        match number.parse::<f64>() {
            Ok(n) if n * multiplier > 0.0 && n * multiplier <= 55.0 => {}
            _ => findings.push(finding(
                "speed_review",
                way,
                format!("maxspeed={value:?}; unsupported code or outside (0, 55] m/s"),
                true,
            )),
        }
    }
}

pub(super) fn stops(
    candidate: &Candidate,
    region: &str,
    network: Option<(&Source, &Network, &BTreeSet<i64>)>,
    fixes: &Fixes<'_>,
    used: &mut BTreeSet<usize>,
    findings: &mut Vec<Finding>,
) -> Result<Vec<Value>> {
    let rows = candidate.data["schedule"]["stops.txt"]
        .as_array()
        .ok_or_else(|| Error::new("GTFS connector lacks stops"))?;
    let feed = &candidate.inventory.source;
    let reachable: BTreeSet<i64> = network
        .into_iter()
        .flat_map(|(_, n, entries)| {
            entries
                .iter()
                .filter_map(|node| n.walk_components.get(node).copied())
        })
        .collect();
    let mut output = vec![];
    for row in rows {
        let f = &row["fields"];
        if super::super::gtfs::location_type(f["location_type"].as_str())? != 0 {
            continue;
        }
        let id = f["stop_id"]
            .as_str()
            .ok_or_else(|| Error::new("GTFS connector lacks stop ID"))?;
        let coord = |name: &str| {
            f[name]
                .as_str()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| Error::new(format!("missing {name} for stop {id}")))
        };
        let coordinates = coordinates(coord("stop_lon")?, coord("stop_lat")?)?;
        let automatic = network.and_then(|(_, n, _)| {
            n.stop_nodes
                .get(&key(coordinates))
                .copied()
                .filter(|node| reachable.contains(&n.walk_components[node]))
        });
        let (node, status, audit) = if let Some(&(index, fix)) = fixes.get(&(region, &feed.id, id))
        {
            let (source, n, _) = network.ok_or_else(|| {
                Error::new(format!("reviewed stop {id} has no network in {region}"))
            })?;
            let separation = n
                .nodes
                .get(&fix.node)
                .map(|node| distance(coordinates, node.coordinates));
            if fix.feed_version != feed.version
                || fix.network != source.id
                || fix.network_version != source.version
                || !n
                    .walk_components
                    .get(&fix.node)
                    .is_some_and(|id| reachable.contains(id))
                || !separation.is_some_and(|d| d <= 100.0)
            {
                return Err(Error::new(format!("stop {id} review needs matching source versions and an eligible walk node within 100 m")));
            }
            used.insert(index);
            findings.push(finding(
                "reviewed_stop_connection",
                id,
                "reviewed alignment; no traversal cost or accessibility inferred",
                false,
            ));
            (
                Some(fix.node),
                "reviewed",
                json!({"fix_index":index,"before_node":automatic,"after_node":fix.node,"separation_m":separation}),
            )
        } else if automatic.is_some() {
            (automatic, "coincident_walk_node", Value::Null)
        } else {
            findings.push(finding("unreachable_stop", id, "no unique coincident pedestrian candidate node connected to a declared walk entry; review paths and barriers", false));
            (None, "unreachable_in_screened_network", Value::Null)
        };
        let target = node.zip(network).map(|(id, (source,n,_))| json!({"source":source.id,"version":source.version,"node":id,"walk_component":n.walk_components[&id]}));
        output.push(json!({"stop_id":id, "source_row":row["source_row"], "coordinates":coordinates,"status":status,"target":target,"review":audit}));
    }
    Ok(output)
}
