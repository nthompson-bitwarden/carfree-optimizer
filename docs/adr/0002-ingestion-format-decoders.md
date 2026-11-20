# ADR 0002. Decode source formats at the ingestion boundary

E03-S01–08, following E02. Proposed for review with issue #3.

Use established Rust decoders in `mobility-ingest` for CSV, XML, JSON,
GTFS-Realtime protobuf and Apache Parquet. These formats have quoting,
escaping, compression or wire-format edge cases. Custom implementations
would duplicate mature parsers and risk accepting malformed source data.
Use the standard GTFS-Realtime schema through generated bindings. Keep
Parquet's Arrow and asynchronous stacks disabled. Keep routing,
optimization and simulation crates independent of these decoders.

Pin direct versions and lock checksummed crates.io resolutions. Update
the dependency check to permit only the reviewed direct declarations in
the ingestion crate and their locked crates.io dependencies. Bootstrap
continues to validate and run offline after the explicit dependency fetch.

Measure representative synthetic input sizes in release mode with the
ingestion CLI. Record file size, row count, wall time and memory where
available. This is offline preparation outside simulation hot paths.

## E20 security maintenance

The XML decoder is pinned to quick-xml 0.41.0 for RustSec advisories 0194
and 0195. The existing Parquet feature set migrates to Arrow Parquet 59.3.3
in both crates, removing the external Thrift 0.17 parser flagged in advisory
0435. Review the lockfile diff and run connector import, normalization replay,
and Parquet export/restore checks before release. The stored workspace and
normalized data schemas remain v1. Keep the previous executable where
byte-identical replay of historical artifacts is required.
