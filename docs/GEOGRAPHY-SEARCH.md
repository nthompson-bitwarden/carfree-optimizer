# Geography search (E02-S01)

[Issue #27](https://github.com/nthompson-bitwarden/carfree-optimizer/issues/27)
implements canonical city/metropolitan-area discovery and boundary display.
Its E01 prerequisite is complete. Saved or drawn study areas and initialization
belong to later E02 stories.

## Run the boundary preview

From the repository root, after [bootstrap](DEVELOPMENT.md):

```sh
cargo run -p mobility-api --bin city-search -- 'Portland' --output /tmp/carfree-boundaries.html
```

Open `/tmp/carfree-boundaries.html` in a browser. The CLI has no network access
requirement. Its default output is standalone HTML on stdout; `--output <path>`
writes a new file and refuses an existing path. Choose another filename when
rerunning. Invalid arguments, invalid queries, and output failures produce
actionable stderr and a nonzero exit status. A failed write attempts to remove
its partial file and reports the cleanup result.

The bundled catalogue supports exactly:

| Canonical ID | Source name | States |
| --- | --- | --- |
| `place:2360545` | Portland city | ME |
| `place:4159000` | Portland city | OR |
| `cbsa:38900` | Portland-Vancouver-Hillsboro, OR-WA Metro Area | OR, WA |

`Portland` displays all three candidates. `Portland ME` returns the Maine city;
`Portland OR` returns the Oregon city and the metro. `cbsa:38900` identifies the
metro. Unknown geography returns an explicit no-match page with the supported
coverage; it never substitutes another city. This pilot is not nationwide
coverage or a live geocoder.

Each matched boundary has its canonical identity, states, source URL, vintage,
retrieval date, license, accessible SVG title/description, and text metadata.
The renderer escapes all interpolated metadata. Source URLs appear as text.
All supplied rings are drawn using even-odd fill, including islands and holes.
The preview is north-up with padded extents and uses longitude/latitude degrees
without a basemap. Each candidate fits its own extent, so displays do not share
a distance scale. It is not a distance/area measurement or topology certification.

## Rust contracts

`mobility_ingest::geography` and crate-root reexports expose:

- `Catalogue::from_tsv(&str) -> Result<Catalogue, CatalogueError>` for arbitrary
  structurally valid catalogues; `Catalogue::pilot()` loads the embedded pilot.
- `entries() -> &[Geography]`, `get(&str) -> Option<&Geography>`, and
  `search(&str) -> Result<Vec<&Geography>, SearchError>`. Catalogue records are
  exposed through shared references, preserving parsed invariants.
- `GeographyKind::{City, MetropolitanArea}` and `Coordinate { longitude: f64,
  latitude: f64 }` in degrees; `Geography` has public `id`, `name`, `kind`,
  `states`, `rings`, `source_url`, `vintage`, `retrieved_date`, and `license`.
- `CatalogueError` includes a one-based `line` and actionable `message`;
  `SearchError::InvalidQuery` distinguishes invalid queries from no matches.
  Both implement `Display` and `std::error::Error`.

Search folds ASCII case, normalizes comma/hyphen/colon separators to spaces,
and AND-matches whitespace-separated tokens across ID, name, and states. A
recognized US state/territory abbreviation must match state metadata exactly;
other tokens prefix-match normalized words. Thus `OR` does not match the letters
inside `Portland`, and `ME` does not match `Metro`. Results sort by canonical ID.
Empty or separator-only queries, any control characters, and queries longer
than 256 UTF-8 bytes return `InvalidQuery`. Search does not perform fuzzy,
accent-insensitive, or relevance-ranked matching.

`mobility_api::geography::render_search_results(&Catalogue, &str)` (also
reexported at the crate root) returns `Result<String, SearchError>`. It renders
every candidate rather than selecting a potentially ambiguous first result.
The CLI uses the embedded pilot; loading custom catalogues is a Rust API operation.

## Catalogue schema v1

UTF-8 TSV begins with the exact line `carfree-geography-v1`. Subsequent records
have these fields, separated by literal tabs:

```text
area    id    city|metro    canonical name    comma-separated states    source URL    vintage    retrieved date    license
ring    longitude,latitude    longitude,latitude    ...
```

The displayed spacing above stands for tabs. Every `area` must be followed by
at least one `ring` before the next area. Blank lines after the header are
allowed. IDs are `place:` plus seven ASCII digits for `city`, or `cbsa:` plus
five digits for `metro`, and must be unique. States are uppercase two-letter
abbreviations separated by commas; the parser validates their form, not their
membership in a geography register. Metadata fields must be nonempty; they
cannot contain tabs or control characters. Source URLs require `https://`, a
nonempty host, and no whitespace. Vintage and retrieval date are preserved
strings, not calendar-validated values.

Each ring contains at least four finite longitude/latitude pairs, includes its
closing vertex, lies within longitude [-180, 180] and latitude [-90, 90], and
has nonzero longitude and latitude extents. CRS is EPSG:4326, serialized in
explicit **longitude, latitude** order. All rings and vertices are retained.
The schema does not classify exterior versus interior rings or certify
orientation, self-intersections, nesting, antimeridian handling, or polygon
topology. A structurally accepted custom catalogue is not proof of a real or
valid study area. Schema version changes require an explicit compatibility
decision; no legacy catalogue format existed before this story.

## Sources, complexity, and validation

The [fixture provenance guide](../fixtures/geography/README.md) documents the
actual Census inputs, transformation, dates, attribution, and regeneration.
The pilot contains source-derived generalized boundaries rather than synthetic
polygons, imputed traffic, or model predictions. There is no calibrated
transportation model or statistical confidence interval in this feature.

Parsing takes expected O(input bytes), with hash-based duplicate-ID detection,
and O(input bytes) storage. `get` scans entries. Search scans normalized catalogue
metadata per query token, then sorts M matches in O(M log M) comparisons; query
size is bounded at 256 bytes. Rendering is linear in matched boundary vertices
plus output metadata, after search. No simulation-critical path or performance
improvement claim is introduced, and no simulation benchmark is implied.

Story-linked acceptance tests:

```sh
cargo test -p mobility-ingest --test e02_s01_geography
cargo test -p mobility-api --test e02_s01_city_search
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/check-dependencies.py
```

Tests cover real canonical identities and source ring coordinates/counts,
ambiguity and state filtering, unsupported/invalid queries, malformed catalogues,
accessible boundary HTML, markup escaping, CLI output, and explicit failures.
Their synthetic malformed/injection records are labeled test data and do not
represent observed geography.
