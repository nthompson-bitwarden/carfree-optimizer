use super::*;
use serde_json::{json, Map};
use std::collections::BTreeMap;

pub(super) fn feature_rows(
    root: &Path,
    path: &Path,
    report: &mut Report,
    mut accept: impl FnMut(u64, Map<String, Value>) -> Result<()>,
) -> Result<()> {
    let mut collection = json_file(root, path)?;
    if collection["type"] != "FeatureCollection" || collection.get("crs").is_some() {
        return Err(Error::new(
            "expected a WGS84 GeoJSON FeatureCollection without a CRS override",
        ));
    }
    let features = collection
        .get_mut("features")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Error::new("missing GeoJSON features array"))?;
    row_limit(features.len())?;
    for (index, feature) in features.iter_mut().enumerate() {
        let line = (index + 1) as u64;
        if let Err(error) = feature_fields(feature).and_then(|fields| accept(line, fields)) {
            report.error(&path.to_string_lossy(), Some(line), error.to_string());
        }
    }
    Ok(())
}

fn feature_fields(feature: &mut Value) -> Result<Map<String, Value>> {
    if feature["type"] != "Feature" || feature.get("crs").is_some() {
        return Err(Error::new(
            "expected a GeoJSON Feature without a CRS override",
        ));
    }
    let geometry = feature
        .get_mut("geometry")
        .ok_or_else(|| Error::new("missing geometry"))?
        .take();
    if !geometry.is_null() {
        validate_geometry(&geometry, 0)?;
    }
    let mut properties = match feature.get_mut("properties").map(Value::take) {
        Some(Value::Object(properties)) => properties,
        Some(Value::Null) => Map::new(),
        _ => return Err(Error::new("properties must be an object or null")),
    };
    if properties.contains_key("@geometry") || properties.contains_key("@id") {
        return Err(Error::new(
            "properties may not use reserved names @geometry or @id",
        ));
    }
    if let Some(id) = feature.get_mut("id") {
        if !(id.is_string() || id.is_number()) {
            return Err(Error::new("feature id must be text or a number"));
        }
        properties.insert("@id".into(), id.take());
    }
    properties.insert("@geometry".into(), geometry);
    Ok(properties)
}

fn position(value: &Value) -> Result<()> {
    let point = array(value)?;
    if !(2..=3).contains(&point.len()) || point.iter().any(|v| !v.is_number()) {
        return Err(Error::new(
            "a position requires longitude, latitude and optional altitude",
        ));
    }
    coordinates(point[0].as_f64().unwrap(), point[1].as_f64().unwrap())?;
    Ok(())
}

fn array(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .filter(|list| !list.is_empty())
        .ok_or_else(|| Error::new("expected a nonempty GeoJSON coordinate array"))
}

fn line(value: &Value) -> Result<()> {
    let points = array(value)?;
    if points.len() < 2 {
        return Err(Error::new("line needs at least two positions"));
    }
    points.iter().try_for_each(position)
}

fn polygon(value: &Value) -> Result<()> {
    for ring in array(value)? {
        let positions = array(ring)?;
        if positions.len() < 4 || positions.first() != positions.last() {
            return Err(Error::new(
                "polygon ring needs at least four positions and closure",
            ));
        }
        positions.iter().try_for_each(position)?;
    }
    Ok(())
}

fn validate_geometry(geometry: &Value, depth: usize) -> Result<()> {
    if depth > 16 || geometry.get("crs").is_some() {
        return Err(Error::new("unsupported GeoJSON geometry nesting or CRS"));
    }
    let coords = &geometry["coordinates"];
    match geometry["type"].as_str() {
        Some("Point") => position(coords),
        Some("MultiPoint") => array(coords)?.iter().try_for_each(position),
        Some("LineString") => line(coords),
        Some("MultiLineString") => array(coords)?.iter().try_for_each(line),
        Some("Polygon") => polygon(coords),
        Some("MultiPolygon") => array(coords)?.iter().try_for_each(polygon),
        Some("GeometryCollection") => {
            let children = geometry["geometries"]
                .as_array()
                .ok_or_else(|| Error::new("expected geometries array"))?;
            children
                .iter()
                .try_for_each(|child| validate_geometry(child, depth + 1))
        }
        _ => Err(Error::new("unsupported GeoJSON geometry")),
    }
}

fn category(properties: &Map<String, Value>) -> &'static str {
    let tag = |name| properties.get(name).and_then(Value::as_str).unwrap_or("");
    match tag("amenity") {
        "school" | "college" | "university" | "kindergarten" | "library" => "education",
        "hospital" | "clinic" | "doctors" | "pharmacy" | "dentist" => "health",
        "marketplace" => "food",
        "restaurant" | "cafe" | "fast_food" => "food_service",
        "townhall" | "post_office" | "community_centre" | "social_facility" => "civic",
        _ if matches!(
            tag("shop"),
            "supermarket" | "convenience" | "greengrocer" | "bakery"
        ) =>
        {
            "food"
        }
        _ if !tag("shop").is_empty() => "retail",
        _ if !tag("healthcare").is_empty() => "health",
        _ if !tag("leisure").is_empty() => "recreation",
        _ => "other",
    }
}

#[derive(Serialize)]
struct Place {
    category: String,
    geometry: Value,
    properties: Map<String, Value>,
    source_id: Option<Value>,
    source_rows: Vec<u64>,
}

pub(super) fn read(root: &Path, path: &Path, report: &mut Report) -> Result<Value> {
    let mut places = Vec::<Place>::new();
    let mut known = BTreeMap::<String, usize>::new();
    feature_rows(root, path, report, |line, mut props| {
        let geometry = props.remove("@geometry").unwrap();
        let id = props.remove("@id");
        let kind = geometry["type"].as_str();
        let category = match kind {
            Some("Point") => category(&props),
            Some("Polygon" | "MultiPolygon") => props
                .get("landuse")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("other"),
            _ => {
                return Err(Error::new(
                    "land use requires Polygon/MultiPolygon and destinations require Point",
                ))
            }
        };
        let key = match &id {
            Some(id) => format!("id:{id}"),
            None => format!("exact:{}", json!([geometry, props])),
        };
        if let Some(&index) = known.get(&key) {
            let prior = &mut places[index];
            if prior.geometry != geometry || prior.properties != props {
                return Err(Error::new(format!(
                    "feature ID conflicts with source row {}",
                    prior.source_rows[0]
                )));
            }
            prior.source_rows.push(line);
        } else {
            known.insert(key, places.len());
            places.push(Place {
                category: category.into(),
                geometry,
                properties: props,
                source_id: id,
                source_rows: vec![line],
            });
        }
        Ok(())
    })?;
    if places.is_empty() && report.errors.is_empty() {
        return Err(Error::new("no land use or destination features"));
    }
    let (destinations, land_use): (Vec<_>, Vec<_>) = places
        .into_iter()
        .partition(|p| p.geometry["type"] == "Point");
    Ok(json!({"destinations":destinations,"land_use":land_use}))
}
