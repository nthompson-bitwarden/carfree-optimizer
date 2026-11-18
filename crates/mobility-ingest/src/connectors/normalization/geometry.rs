use super::*;
use crate::{study_area::geometry::validate, Coordinate};

/// Bound the aggregate quadratic work before invoking the shared polygon validator.
const PAIR_BUDGET: usize = 50_000_000;

pub(super) fn screen(data: &Value, findings: &mut Vec<Finding>) -> Result<bool> {
    let land = data["land_use"]
        .as_array()
        .ok_or_else(|| Error::new("places connector lacks land use collection"))?;
    let mut budget = PAIR_BUDGET;
    let mut valid = true;
    for feature in land {
        let geometry = &feature["geometry"];
        let coords = &geometry["coordinates"];
        let record = format!("source rows {}", feature["source_rows"]);
        let parts = match geometry["type"].as_str() {
            Some("Polygon") => std::slice::from_ref(coords),
            Some("MultiPolygon") => coords
                .as_array()
                .map(Vec::as_slice)
                .ok_or_else(|| Error::new("missing MultiPolygon parts"))?,
            _ => return Err(Error::new("unexpected land-use geometry type")),
        };
        for part in parts {
            let rings = part
                .as_array()
                .ok_or_else(|| Error::new("missing Polygon rings"))?;
            let count = rings
                .iter()
                .map(|r| r.as_array().map_or(0, Vec::len))
                .sum::<usize>();
            if count > 10_000 || count.saturating_mul(count) > budget {
                findings.push(finding("polygon_screening_limit", &record, "polygon exceeds bounded topology check; split or simplify the source explicitly", true));
                return Ok(false);
            }
            budget -= count * count;
            let rings = rings
                .iter()
                .map(|ring| {
                    ring.as_array()
                        .ok_or_else(|| Error::new("missing ring positions"))?
                        .iter()
                        .map(|p| {
                            let (Some(longitude), Some(latitude)) = (p[0].as_f64(), p[1].as_f64())
                            else {
                                return Err(Error::new("missing polygon coordinate"));
                            };
                            Ok(Coordinate {
                                longitude,
                                latitude,
                            })
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<Vec<_>>>()?;
            if let Err(error) = validate(&rings) {
                findings.push(finding("invalid_polygon", &record, error, true));
                valid = false;
            }
        }
        if parts.len() > 1 {
            findings.push(finding(
                "multipart_topology_review",
                &record,
                "individual polygons checked; overlap and contacts between parts require review",
                false,
            ));
        }
    }
    Ok(valid)
}
