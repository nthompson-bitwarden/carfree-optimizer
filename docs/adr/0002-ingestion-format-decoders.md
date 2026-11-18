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
