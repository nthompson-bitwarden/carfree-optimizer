# E02-S01 Census geography pilot provenance

`pilot.tsv` is the embedded `carfree-geography-v1` catalogue.
`sources.json` uses `carfree-geography-source-v1` and records source URLs,
archive/member SHA256 digests, canonical names and IDs, ring vertex counts,
catalogue SHA256, CRS, units, scale, vintage, retrieval date, and attribution.
The [search/schema contract](../../docs/GEOGRAPHY-SEARCH.md) describes runtime use.

## Source and coverage

Source: U.S. Census Bureau,
[2024 cartographic boundary files](https://www.census.gov/geographies/mapping-files/2024/geo/carto-boundary-file.html),
KML at **1:500,000** scale, vintage **2024**, retrieved **2026-11-17 UTC**.
These are generalized source geographic boundaries, not invented polygons or
observed transportation measurements. They are unsuitable for cadastral/legal
boundary decisions or unqualified engineering analysis. No accuracy confidence
interval or transportation calibration is supplied by this pilot.

| Input archive | Selected source GEOID / NAMELSAD | States | Rings / vertices |
| --- | --- | --- | --- |
| [Maine places](https://www2.census.gov/geo/tiger/GENZ2024/kml/cb_2024_23_place_500k.zip) | `2360545` / Portland city | ME | 6 / 197 |
| [Oregon places](https://www2.census.gov/geo/tiger/GENZ2024/kml/cb_2024_41_place_500k.zip) | `4159000` / Portland city | OR | 9 / 666 |
| [US core-based statistical areas](https://www2.census.gov/geo/tiger/GENZ2024/kml/cb_2024_us_cbsa_500k.zip) | `38900` / Portland-Vancouver-Hillsboro, OR-WA Metro Area | OR, WA | 1 / 1377 |

The pilot includes only these three areas. Other source archive records are
excluded explicitly; no nationwide coverage is claimed. No individual-level
records or personal travel data are included.

The attribution field is `U.S. Census Bureau public domain; attribution requested`.
See the Census Bureau's [citation guidance](https://www.census.gov/about/policies/citation.html)
and [public-access policy](https://www2.census.gov/foia/ds_policies/ds027.pdf)
on government-created works. Source data attribution is distinct from the
repository code's MIT license.

## Reproduce without runtime downloads

Download the three linked archives into a local directory without renaming
them. Raw nationwide/state archives are not committed. For example:

```sh
mkdir -p /tmp/carfree-geography-sources
curl -fL https://www2.census.gov/geo/tiger/GENZ2024/kml/cb_2024_23_place_500k.zip -o /tmp/carfree-geography-sources/cb_2024_23_place_500k.zip
curl -fL https://www2.census.gov/geo/tiger/GENZ2024/kml/cb_2024_41_place_500k.zip -o /tmp/carfree-geography-sources/cb_2024_41_place_500k.zip
curl -fL https://www2.census.gov/geo/tiger/GENZ2024/kml/cb_2024_us_cbsa_500k.zip -o /tmp/carfree-geography-sources/cb_2024_us_cbsa_500k.zip
python3 scripts/prepare-geography.py /tmp/carfree-geography-sources --check
```

Run from the repository root with Python 3.11 or newer. `--check` compares
regenerated catalogue and manifest bytes with the committed artifacts and
does not write. To inspect regeneration separately:

```sh
python3 scripts/prepare-geography.py /tmp/carfree-geography-sources --output-dir /tmp/carfree-geography-regenerated
```

Without `--check` or `--output-dir`, the default output directory is this
`fixtures/geography` directory and its two generated files are replaced.
The helper never downloads data. Missing files, changed archive/member hashes,
missing/duplicate selected IDs, changed canonical names/states, unexpected metro
classification, or malformed source rings fail explicitly.

The helper reads exact `.kml` members, selects `GEOID` and verifies `NAMELSAD`.
City state metadata comes from verified `STUSPS`; metro states are explicitly
selected from the verified `OR-WA` name suffix, and `LSAD` must be metropolitan
`M1`. Canonical IDs add `place:` or `cbsa:` according to the selected layer.
Every source `LinearRing` and its original longitude/latitude decimal strings
are retained, including islands and holes. Zero KML altitude is checked and
discarded. No clipping, reprojection, manual simplification, or inferred
transportation input is applied. KML WGS84 longitude/latitude is recorded as
EPSG:4326 degrees in longitude-first serialization order. The Rust parser then
stores coordinates as `f64`; HTML/SVG renders a schematic preview with six
decimal places in display coordinates, leaving the source catalogue intact.

Pinned digests mean a future Census replacement of the same filename will
fail verification. Updating vintage or source artifacts requires deliberate
manifest, helper, catalogue, regression-test, and documentation review.
