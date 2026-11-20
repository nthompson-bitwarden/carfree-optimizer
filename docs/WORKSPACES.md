# Saved workspaces and exports

E19 adds an opt-in Rust HTTP API to the spatial workbench. It saves a dated
Census study-area snapshot, immutable ordered scenario revisions, and the
associated evaluation status and results. The bundled evaluator uses the E17
and E18 synthetic network, services, cohorts and demand. The Census area
identifies the project and supplies its onboarding boundary and coverage
inventory. It has no transport-network correspondence to the synthetic runs.

## Run the review demo

From the repository root, create a private store and bootstrap credential.
Use a new directory for this example. To reopen later, keep that directory,
the credential and the exact workbench executable. On Unix use mode 700
for the store and mode 600 for the owner credential file.

```sh
umask 077
mkdir -p .codex/run
mkdir -m 700 .codex/run/e19-demo
python3 -c 'import secrets; print(secrets.token_hex(32))' > .codex/run/e19-demo/owner
printf 'Authorization: Bearer %s\n' "$(cat .codex/run/e19-demo/owner)" > .codex/run/e19-demo/auth
rustup run 1.97.1 cargo run --locked --offline -p mobility-api --bin spatial-workbench -- \
  8047 .codex/run/e19-demo .codex/run/e19-demo/owner
```

In a second terminal at the repo root, create a project and run its first
revision. The paths below assume an empty store. Use the actual returned IDs
when working with an existing store.

```sh
api=http://127.0.0.1:8047/api/e19/v1
auth=.codex/run/e19-demo/auth

curl -fsS "$api/cities"
curl -fsS -H "@$auth" --json '{"title":"Portland study","area":"place:4159000","date":"2026-01-05"}' \
  "$api/projects"
curl -fsS -H "@$auth" --json '{"title":"Walkway","plan":{"projects":[0],"width":1900,"capital":200,"annual":30}}' \
  "$api/projects/1/scenarios"

revision="$api/projects/1/scenarios/1/revisions/1"
curl -fsS -H "@$auth" -X POST "$revision/jobs"
curl -fsS -H "@$auth" "$revision/jobs"
curl -fsS -H "@$auth" "$api/projects/1"
for artifact in scenario.json metrics.parquet manifest.json map.svg report.html; do
  curl -fsS -H "@$auth" "$revision/exports/$artifact" -o ".codex/run/e19-demo/$artifact"
done
```

Open the downloaded `map.svg` and `report.html` in a browser. The map shows
the final phase's directed network, absent layers and unknown link access. The
report includes nominal metrics, sensitivity ranges and screened trade-offs.
Both embed source, version, fixture date, license and caveats. The schematic
uses dimensionless coordinates. Metric units are independent of its map scale.

Restart the same executable with the same arguments. Retrieve the saved
project and revision again. The exact E02 project TSV is in `city.projectTsv`.
The recorded build SHA-256 pins embedded topology, demand and model code.
Keep an immutable copy of the executable if future replay is required. A
changed build yields `stale: "evaluator_changed"`; an inconsistent city snapshot or display projection
yields `"saved_city_snapshot_changed"`. Current inputs yield `null`. Stale
projects can still reopen saved results and export them. New scenario revisions
and evaluations require the recorded inputs. A fresh project can use a new
build. The server does not migrate projects onto new inputs. On reopen, it verifies
completed evaluation and share payload digests. Restore a known-good backup
if the ledger fails an integrity check.

## API and storage

[OpenAPI 3.1](workspaces/openapi.json) is served from
`/api/e19/v1/openapi.json`. Cities, scenarios, revisions, evaluation jobs,
results and exports have versioned routes. JSON errors include `code` and
`message`. HTTP framing errors use the workbench `error` field. Send
`Content-Type: application/json` with JSON writes. Requests have a 16 KiB
body and 8 KiB header limit. City reads and extension compatibility are
anonymous. Job submission is a bodyless POST which evaluates a bounded plan
synchronously and stores its final status before returning. Resubmitting a
completed revision returns its existing result. A failed job retains its
diagnostic. A completed computation whose final write fails reports failure.
If storage is unavailable to save that failure, reopening reports the last
durable running record as interrupted. A process interrupted during an evaluation yields an
`interrupted` job on reopen and can be retried with the same inputs. E16's
distributed job coordinator serves seeded batch experiments separately.
See [experiment jobs](EXPERIMENT-JOBS.md).

The ledger is `workspace-v1.json` inside the configured private local
directory. A filesystem lock permits one serving process. Each write stages
and flushes a replacement file, then renames it over the previous ledger.
Use a private local filesystem and back up the store and executable. There
is a 32 MiB ledger limit. The workbench has no workspace quota, remote
worker identity or hosted tenant model. The ordinary E17 browser continues
to use the original ephemeral interface.

Exports are projections of the saved evaluation. `scenario.json` includes
the ordered intervention plan, phase networks and full E18 report.
`metrics.parquet` has one row per metric and nominal phase, with `phase`,
`metric`, `unit`, `value`, `incremental` and `cumulative`. Unavailable values
are null. Phase zero is baseline. File metadata embeds the manifest and
the exact plan as JSON. The separately downloadable manifest binds the
onboarding snapshot, build, scenario and report by SHA-256 and records
schema versions, seed and units. The scenario and report digests hash the
compact UTF-8 JSON object bytes with lexically ordered object keys as
emitted by the v1 exporter.

## Collaborate and publish

The bootstrap credential is the workspace operator and owner of each
project. The owner can create projects, grant project-scoped editor and viewer
credentials, decide reviews and publish or revoke shares. An editor can add
scenarios, append revisions, run evaluations and comment. A viewer can read
project data and export its saved results. The server returns a member's
bearer credential only when issued. Treat the credential as private.

```sh
curl -fsS -H "@$auth" --json '{"role":"editor"}' "$api/projects/1/members"
curl -fsS -H "@$auth" --json '{"text":"Review the modeled schedule coverage."}' "$revision/comments"
curl -fsS -H "@$auth" -X PUT --json '{"state":"in_review"}' "$revision/review"
curl -fsS -H "@$auth" --json '{"projection":"report"}' "$revision/shares"
```

A review can be `draft`, `in_review`, `approved` or `changes_requested`.
Entering or leaving an owner decision requires the owner. Comments and
review states stay on their original revision when a new revision is added.

Open the returned share `path` on the printed server origin without an
authorization header. Each capability exposes exactly one frozen projection
of the selected revision: `scenario` publishes only the plan and caveat,
`report` publishes standalone HTML, and `map` publishes standalone SVG.
Workspace titles, members, city onboarding, comments, job status and other
revisions stay private. Use the returned share ID with
`DELETE /projects/{project}/shares/{share}` to revoke access. Previously
downloaded copies remain with recipients. Share URLs are bearer capabilities.
The server binds IPv4 loopback and is intended for a local machine or a
trusted tunnel. For recipients outside that environment, distribute the
standalone artifacts through an authorized channel.

## Extend

[Extension manifests](workspaces/extensions.json) document compatible
connector and optimizer exchanges and include runnable examples. POST either
example to `/extensions/validate` to check protocol version and sample
compatibility. Dataset connectors emit a mapped upload request accepted by the
E03 ingest workflow. Compatibility checks metadata and mapping shape without
opening sample files. The existing ingest command validates actual data and
requires explicit apply. Optimizers consume versioned workspace exports and
propose ordered plans, checked against the same Rust screening as the
workbench. Extensions execute outside the server.

## Evidence and limits

The Rust tests labeled E19-S01 through E19-S08 cover restart and staleness,
request validation, saved job status, decoded Parquet parity, roles, scope,
publication, standalone artifacts, compatibility, and revision discussion.
Storage serializes the ledger in O(saved bytes) per write; the Parquet metric
export is O(phases × metrics). These are application and export paths. The
existing routing, assignment, demand and optimization algorithms are unchanged.

On macOS arm64 with Rust 1.97.1 in release mode, six two-step synthetic
revisions in a 379 KB local ledger took a median 30.0 ms per loopback
evaluation with storage, with a 28.1–36.2 ms range. Median revision append
was 9.47 ms across five writes. Median Parquet export was 0.38 ms across
six exports. The fixture uses the Portland, OR onboarding snapshot as
project metadata and the synthetic network for all six evaluations.

The synthetic scenarios preserve the model's service windows, wait
assumptions, mobility constraints and infeasible essential-trip metrics.
They provide no observed calibration or empirical uncertainty. Review
approval records a collaborator's decision about a revision. Real-world
planning recommendations require representative inputs and independent
calibration and uncertainty evidence.
