# ADR 0006. Use existing codecs for versioned workspace storage and export

E19-S01 through E19-S08, issue #19.

Use the versions of Serde, serde_json, SHA-256, tempfile, getrandom, and
Parquet already resolved in the workspace lockfile. Use serde_json raw-value decoding for workspace projections so they
preserve exact floating-point values with Rust parsing. Ingestion retains
its original decimal parser and existing source and snapshot identities. Workspace persistence
requires typed JSON and atomic staging. Executable, snapshot and export
identities require stable hashes. Membership and share capabilities require
OS randomness. Parquet requires interoperable binary encoding with nullable
numeric values. Enable no Parquet reader/writer features beyond those already
needed by ingestion. Add no dependencies to the scientific engines.

The bounded reference store uses a process lock and atomic local JSON
replacement. Readable backups and restart behavior matter more than write
throughput at this scale. Per-write work grows with the store, so cap it at
32 MiB. An embedded database can be reconsidered against a measured
multi-workspace workload and migration requirements.

The Rust dependency gate permits only the exact direct declarations in
`mobility-api` and retains the locked-checksum checks. The E19 workspace
demo and benchmark measure persistence, evaluation, and export outside
the routing and simulation hot paths. See [Workspaces](../WORKSPACES.md).
