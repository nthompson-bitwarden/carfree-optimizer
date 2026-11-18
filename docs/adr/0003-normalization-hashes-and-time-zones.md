# ADR 0003: Use pinned SHA-256 and IANA time zone rules for E04

- **Status:** Accepted for offline normalization
- **Scope:** `mobility-ingest` only; no API, routing, or simulator dependencies

## Decision

Use `sha2 = 0.11.1` for content addressing and verification, not a standard
library implementation-dependent hash or an external executable. Use
`chrono-tz = 0.10.4` with default features disabled to evaluate GTFS service
clock instants deterministically using bundled IANA rules; record the tzdb
version in the normalized schedule. Host tzdata can differ between installs,
and UTC-offset arithmetic cannot model daylight-saving transitions. Do not
implement either SHA-256 or tzdb rules in project code.

The direct versions are pinned, transitive versions and checksums are locked,
and the dependency policy approves these exact requirements for ingest only.
After dependency fetch, neither library needs network access. `sha2` uses
MIT/Apache-2.0; `chrono-tz` uses MIT/Apache-2.0 and embeds public-domain IANA
zone rules. Updating either dependency requires checking replay compatibility;
if derived bytes change, bump the processing version.

See the [E04 benchmark](../BENCHMARKS.md#e04-normalization) for end-to-end cost
including hashing. This is not a throughput or security guarantee.
