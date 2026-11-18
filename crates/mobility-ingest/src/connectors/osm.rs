use super::*;
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::BufReader,
};

type Tags = BTreeMap<String, String>;

#[derive(Default)]
struct Extract {
    nodes: BTreeMap<i64, Node>,
    ways: BTreeMap<i64, Way>,
    relations: BTreeMap<i64, Relation>,
}

#[derive(Serialize)]
struct Node {
    coordinates: [f64; 2],
    tags: Tags,
}

struct Way {
    refs: Vec<i64>,
    tags: Tags,
}

#[derive(Serialize)]
struct Relation {
    members: Vec<Member>,
    tags: Tags,
}

#[derive(Serialize)]
struct Member {
    #[serde(rename = "type")]
    kind: String,
    id: i64,
    role: String,
}

enum Object {
    Node(i64, Node),
    Way(i64, Way),
    Relation(i64, Relation),
}

fn xml_error(error: impl fmt::Display) -> Error {
    Error::new(format!("OSM XML: {error}"))
}

fn attrs(reader: &Reader<impl std::io::BufRead>, start: &BytesStart<'_>) -> Result<Tags> {
    let mut fields = Tags::new();
    for item in start.attributes() {
        let attr = item.map_err(xml_error)?;
        let key = std::str::from_utf8(attr.key.as_ref()).map_err(xml_error)?;
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(xml_error)?;
        fields.insert(key.to_owned(), value.into_owned());
    }
    Ok(fields)
}

fn get<'a>(attr: &'a Tags, key: &str) -> Result<&'a str> {
    attr.get(key)
        .map(String::as_str)
        .ok_or_else(|| xml_error(format!("missing {key} attribute")))
}

fn id(attr: &Tags, key: &str) -> Result<i64> {
    let value = get(attr, key)?.parse::<i64>().map_err(xml_error)?;
    if value == 0 {
        return Err(xml_error("IDs and references must be nonzero"));
    }
    Ok(value)
}

impl Object {
    fn new(name: &[u8], attr: &Tags) -> Result<Option<Self>> {
        let object = match name {
            b"node" => {
                let lon = get(attr, "lon")?.parse().map_err(xml_error)?;
                let lat = get(attr, "lat")?.parse().map_err(xml_error)?;
                Self::Node(
                    id(attr, "id")?,
                    Node {
                        coordinates: coordinates(lon, lat)?,
                        tags: Tags::new(),
                    },
                )
            }
            b"way" => Self::Way(
                id(attr, "id")?,
                Way {
                    refs: Vec::new(),
                    tags: Tags::new(),
                },
            ),
            b"relation" => Self::Relation(
                id(attr, "id")?,
                Relation {
                    members: Vec::new(),
                    tags: Tags::new(),
                },
            ),
            b"bounds" => return Ok(None),
            _ => return Err(xml_error("unexpected top-level element")),
        };
        Ok(Some(object))
    }

    fn child(&mut self, name: &[u8], attr: &Tags) -> Result<()> {
        let tags = match self {
            Self::Way(_, way) if name == b"nd" => {
                way.refs.push(id(attr, "ref")?);
                return row_limit(way.refs.len());
            }
            Self::Relation(_, relation) if name == b"member" => {
                relation.members.push(Member {
                    kind: get(attr, "type")?.to_owned(),
                    id: id(attr, "ref")?,
                    role: get(attr, "role")?.to_owned(),
                });
                return row_limit(relation.members.len());
            }
            Self::Way(_, way) => &mut way.tags,
            Self::Relation(_, relation) => &mut relation.tags,
            Self::Node(_, node) => &mut node.tags,
        };
        if name != b"tag" {
            return Err(xml_error("unexpected OSM object child"));
        }
        let key = get(attr, "k")?;
        if key.is_empty()
            || tags
                .insert(key.to_owned(), get(attr, "v")?.to_owned())
                .is_some()
        {
            return Err(xml_error(format!("empty or duplicate tag key {key}")));
        }
        Ok(())
    }

    fn finish(self, output: &mut Extract, seen: &mut BTreeSet<(u8, i64)>) -> Result<()> {
        let key = match &self {
            Self::Node(id, _) => (0, *id),
            Self::Way(id, _) => (1, *id),
            Self::Relation(id, _) => (2, *id),
        };
        if !seen.insert(key) {
            return Err(xml_error(format!("duplicate OSM object ID {}", key.1)));
        }
        row_limit(seen.len())?;
        match self {
            Self::Node(id, point) => {
                output.nodes.insert(id, point);
            }
            Self::Way(id, way) if way.tags.contains_key("highway") => {
                output.ways.insert(id, way);
            }
            Self::Relation(id, rel)
                if rel
                    .tags
                    .get("type")
                    .is_some_and(|tag| tag == "restriction" || tag.starts_with("restriction:")) =>
            {
                output.relations.insert(id, rel);
            }
            _ => {}
        }
        Ok(())
    }
}

fn parse(root: &Path, path: &Path) -> Result<Extract> {
    let mut reader =
        Reader::from_reader(BufReader::new(open(root, path)?.take(MAX_INPUT_BYTES + 1)));
    reader.config_mut().trim_text(true);
    let mut output = Extract::default();
    let mut buffer = Vec::new();
    let mut seen = BTreeSet::new();
    let (mut depth, mut root_seen) = (0, false);
    let mut object = None;
    loop {
        let event = reader.read_event_into(&mut buffer).map_err(xml_error)?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let empty = matches!(event, Event::Empty(_));
                let name = start.name();
                let attr = attrs(&reader, start)?;
                match depth {
                    0 if !root_seen
                        && name.as_ref() == b"osm"
                        && get(&attr, "version")? == "0.6" =>
                    {
                        root_seen = true
                    }
                    1 => {
                        object = Object::new(name.as_ref(), &attr)?;
                        if empty {
                            if let Some(item) = object.take() {
                                item.finish(&mut output, &mut seen)?;
                            }
                        }
                    }
                    2 => object
                        .as_mut()
                        .ok_or_else(|| xml_error("unexpected nested element"))?
                        .child(name.as_ref(), &attr)?,
                    _ => return Err(xml_error("expected a single OSM 0.6 extract")),
                }
                depth += usize::from(!empty);
            }
            Event::End(_) => {
                if depth == 0 {
                    return Err(xml_error("unmatched closing tag"));
                }
                depth -= 1;
                if depth == 1 {
                    if let Some(item) = object.take() {
                        item.finish(&mut output, &mut seen)?;
                    }
                }
            }
            Event::Eof if root_seen && depth == 0 => break,
            Event::Decl(_) if !root_seen => {}
            Event::Comment(_) => {}
            Event::Text(text) if text.is_empty() => {}
            _ => return Err(xml_error("unexpected or incomplete XML content")),
        }
        if reader.buffer_position() > MAX_INPUT_BYTES {
            return Err(xml_error("file grew beyond the size limit"));
        }
        buffer.clear();
    }
    Ok(output)
}

fn restriction(
    relation: &Relation,
    ways: &BTreeMap<i64, Way>,
    nodes: &BTreeMap<i64, Node>,
) -> Result<()> {
    if !relation
        .tags
        .keys()
        .any(|key| key == "restriction" || key.starts_with("restriction:"))
    {
        return Err(Error::new("missing restriction tag"));
    }
    let mut from = Vec::new();
    let mut to = Vec::new();
    let mut via = Vec::new();
    for member in &relation.members {
        match (member.role.as_str(), member.kind.as_str()) {
            ("from", "way") if ways.contains_key(&member.id) => from.push(member.id),
            ("to", "way") if ways.contains_key(&member.id) => to.push(member.id),
            ("via", "way") if ways.contains_key(&member.id) => via.push((false, member.id)),
            ("via", "node") if nodes.contains_key(&member.id) => via.push((true, member.id)),
            _ => {
                return Err(Error::new(format!(
                    "unresolved or unsupported {} {} {}",
                    member.role, member.kind, member.id
                )))
            }
        }
    }
    if from.is_empty()
        || to.is_empty()
        || via.is_empty()
        || (via.len() > 1 && via.iter().any(|(is_node, _)| *is_node))
    {
        return Err(Error::new(
            "restriction requires from/to ways and one via node or ordered via ways",
        ));
    }
    if let [(true, node)] = via.as_slice() {
        if from
            .iter()
            .chain(&to)
            .any(|way| !ways[way].refs.contains(node))
        {
            return Err(Error::new("via node does not lie on every from/to way"));
        }
    } else {
        let shared_node = |a: i64, b: i64| {
            let refs: BTreeSet<_> = ways[&a].refs.iter().collect();
            ways[&b].refs.iter().any(|node| refs.contains(node))
        };
        if from.iter().any(|way| !shared_node(*way, via[0].1))
            || via
                .windows(2)
                .any(|pair| !shared_node(pair[0].1, pair[1].1))
            || to
                .iter()
                .any(|way| !shared_node(via.last().unwrap().1, *way))
        {
            return Err(Error::new(
                "ordered via ways are disconnected from one another or the from/to ways",
            ));
        }
    }
    Ok(())
}

pub(super) fn read(root: &Path, path: &Path, report: &mut Report) -> Result<Value> {
    let extract = parse(root, path)?;
    let label = path.to_string_lossy();
    let mut ways = BTreeMap::new();
    let mut segment_count = 0;
    let mut nodes = BTreeMap::new();
    for (id, way) in &extract.ways {
        if way.refs.len() < 2 || way.tags["highway"].is_empty() {
            report.error(
                &label,
                None,
                format!("way {id} needs a highway class and at least two nodes"),
            );
            continue;
        }
        segment_count += way.refs.len() - 1;
        row_limit(segment_count)?;
        for node_id in &way.refs {
            if let Some(node) = extract.nodes.get(node_id) {
                nodes.insert(node_id.to_string(), node);
            }
        }
        let mut segments = Vec::new();
        for (index, pair) in way.refs.windows(2).enumerate() {
            match (extract.nodes.get(&pair[0]), extract.nodes.get(&pair[1])) {
                (Some(from), Some(to)) if pair[0] != pair[1] => segments.push(json!({
                    "index":index, "from":{"id":pair[0],"coordinates":from.coordinates},
                    "to":{"id":pair[1],"coordinates":to.coordinates}
                })),
                _ => report.error(
                    &label,
                    None,
                    format!(
                        "way {id} segment {index} has a repeated or unresolved node ({} to {})",
                        pair[0], pair[1]
                    ),
                ),
            }
        }
        ways.insert(id.to_string(), json!({"tags":way.tags,"segments":segments}));
    }
    for (id, relation) in &extract.relations {
        if let Err(error) = restriction(relation, &extract.ways, &extract.nodes) {
            report.error(&label, None, format!("restriction relation {id}: {error}"));
        }
    }
    if ways.is_empty() && report.errors.is_empty() {
        return Err(Error::new("no highway ways in OSM extract"));
    }
    let restrictions: BTreeMap<_, _> = extract
        .relations
        .iter()
        .map(|(id, rel)| (id.to_string(), rel))
        .collect();
    Ok(json!({"ways":ways, "nodes":nodes, "restrictions":restrictions}))
}
