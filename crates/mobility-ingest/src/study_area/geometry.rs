use super::{err, Coordinate, OnboardingError};
pub(super) fn extent(rings: &[Vec<Coordinate>]) -> [f64; 4] {
    rings.iter().flatten().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[w, s, e, n], p| {
            [
                w.min(p.longitude),
                s.min(p.latitude),
                e.max(p.longitude),
                n.max(p.latitude),
            ]
        },
    )
}
fn cross(a: Coordinate, b: Coordinate, c: Coordinate) -> f64 {
    (b.longitude - a.longitude) * (c.latitude - a.latitude)
        - (b.latitude - a.latitude) * (c.longitude - a.longitude)
}
fn on(a: Coordinate, b: Coordinate, p: Coordinate) -> bool {
    cross(a, b, p) == 0.0
        && p.longitude >= a.longitude.min(b.longitude)
        && p.longitude <= a.longitude.max(b.longitude)
        && p.latitude >= a.latitude.min(b.latitude)
        && p.latitude <= a.latitude.max(b.latitude)
}
fn intersects(a: Coordinate, b: Coordinate, c: Coordinate, d: Coordinate) -> bool {
    let (x, y, z, w) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (x.signum() != y.signum()
        && x != 0.0
        && y != 0.0
        && z.signum() != w.signum()
        && z != 0.0
        && w != 0.0)
        || on(a, b, c)
        || on(a, b, d)
        || on(c, d, a)
        || on(c, d, b)
}
// None indicates boundary. Ray casting is used only after crossing validation.
fn inside(p: Coordinate, r: &[Coordinate]) -> Option<bool> {
    let mut hit = false;
    for edge in r.windows(2) {
        let (a, b) = (edge[0], edge[1]);
        if on(a, b, p) {
            return None;
        }
        if (a.latitude > p.latitude) != (b.latitude > p.latitude)
            && p.longitude
                < (b.longitude - a.longitude) * (p.latitude - a.latitude)
                    / (b.latitude - a.latitude)
                    + a.longitude
        {
            hit = !hit
        }
    }
    Some(hit)
}
pub(crate) fn validate(rings: &[Vec<Coordinate>]) -> Result<(), OnboardingError> {
    if rings.iter().map(Vec::len).sum::<usize>() > 10_000 {
        return Err(err("at most 10000 polygon vertices are supported"));
    }
    if rings.is_empty() {
        return Err(err("at least one closed polygon ring is required"));
    }
    // Quadratic segment-pair checks. No tolerance snaps or coordinate modification.
    for r in rings {
        if r.len() < 4 || r.first() != r.last() {
            return Err(err(
                "rings require at least four vertices and exact closure",
            ));
        }
        if r.iter().any(|p| {
            !p.longitude.is_finite()
                || !p.latitude.is_finite()
                || p.longitude <= -180.0
                || p.longitude >= 180.0
                || p.latitude <= -90.0
                || p.latitude >= 90.0
        }) {
            return Err(err("finite coordinates within (-180,180) longitude and (-90,90) latitude required; polar/dateline geometries unsupported"));
        }
        if r.windows(2)
            .any(|p| p[0] == p[1] || (p[0].longitude - p[1].longitude).abs() >= 180.0)
        {
            return Err(err("duplicate adjacent vertices or antimeridian crossing"));
        }
        let origin = r[0];
        let area: f64 = r.windows(2).map(|p| cross(origin, p[0], p[1])).sum();
        if area == 0.0 || !area.is_finite() {
            return Err(err("polygon ring has zero or invalid area"));
        }
        for i in 0..r.len() - 1 {
            for j in i + 1..r.len() - 1 {
                if j == i + 1 || i == 0 && j == r.len() - 2 {
                    continue;
                }
                if intersects(r[i], r[i + 1], r[j], r[j + 1]) {
                    return Err(err("polygon ring self-intersects or self-touches"));
                }
            }
        }
        // Adjacent edges must not double back over one another.
        for i in 0..r.len() - 1 {
            let a = r[(i + r.len() - 2) % (r.len() - 1)];
            let b = r[i];
            let c = r[i + 1];
            if cross(a, b, c) == 0.0 && (on(a, b, c) || on(b, c, a)) {
                return Err(err("adjacent polygon edges overlap"));
            }
        }
    }
    let mut contacts: Vec<(usize, usize, Coordinate)> = Vec::new();
    for i in 0..rings.len() {
        for j in i + 1..rings.len() {
            let (a, b) = (&rings[i], &rings[j]);
            let mut shared: Option<Coordinate> = None;
            for x in a.windows(2) {
                for y in b.windows(2) {
                    if !intersects(x[0], x[1], y[0], y[1]) {
                        continue;
                    }
                    let common = if x[0] == y[0] || x[0] == y[1] {
                        Some(x[0])
                    } else if x[1] == y[0] || x[1] == y[1] {
                        Some(x[1])
                    } else {
                        None
                    };
                    let p = common.ok_or_else(|| {
                        err("polygon rings cross, overlap, or touch away from a shared vertex")
                    })?;
                    let other_x = if x[0] == p { x[1] } else { x[0] };
                    let other_y = if y[0] == p { y[1] } else { y[0] };
                    if on(p, other_x, other_y) || on(p, other_y, other_x) {
                        return Err(err("polygon rings share an edge"));
                    }
                    if shared.is_some_and(|q| q != p) {
                        return Err(err("multiple contacts between rings are unsupported"));
                    }
                    shared = Some(p);
                }
            }
            if let Some(p) = shared {
                // A tangent ring's non-contact vertices must be wholly inside or outside.
                for (r, other) in [(a, b), (b, a)] {
                    let mut state = None;
                    for p in r.iter().take(r.len() - 1) {
                        if let Some(value) = inside(*p, other) {
                            if state.is_some_and(|v| v != value) {
                                return Err(err("rings cross at their shared vertex"));
                            }
                            state = Some(value)
                        }
                    }
                    if state.is_none() {
                        return Err(err("ambiguous coincident rings"));
                    }
                }
                contacts.push((i, j, p));
            }
        }
    }
    // A cycle of contacts can disconnect polygon interior. Support a forest only.
    let mut parent: Vec<usize> = (0..rings.len()).collect();
    fn root(p: &[usize], mut i: usize) -> usize {
        while p[i] != i {
            i = p[i]
        }
        i
    }
    for (i, j, p) in &contacts {
        if contacts.iter().filter(|(_, _, q)| q == p).count() > 1 {
            return Err(err("more than two rings meet at one point"));
        }
        let (a, b) = (root(&parent, *i), root(&parent, *j));
        if a == b {
            return Err(err("ring contacts form an unsupported cycle"));
        }
        parent[a] = b;
    }
    Ok(())
}
