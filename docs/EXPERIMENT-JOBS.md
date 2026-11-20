# E16 durable synthetic experiment jobs (v1)

This is an operator/researcher CLI, not an HTTP application service or a regional
experiment deployment. Python 3.11+ standard-library orchestration lives in
`training/experiment_jobs.py`; **all simulation, guards, masks and rewards remain
in the existing Rust E14/E15 evaluator**. No new dependencies or scientific models.
Native coordinator/client use is portable; evaluator execution is qualified only
on **64-bit Linux**. Unsupported workers refuse work, with an actionable log.

## Reviewable demo

Build on a Linux worker (install the pinned toolchain first):

```sh
cargo build --locked -p optimizer-bridge --example e16_evaluate
mkdir -p .codex/run/e16
umask 077
python3 -c 'import secrets; print(secrets.token_hex(32))' > .codex/run/e16/token
python3 training/experiment_jobs.py --token-file .codex/run/e16/token spec \
  --evaluator target/debug/examples/e16_evaluate --seeds 7 19 31 \
  > .codex/run/e16/spec.json
python3 training/experiment_jobs.py --token-file .codex/run/e16/token serve \
  --database .codex/run/e16/jobs.sqlite --capacity 2
```

In another terminal, submit once, copy the returned job ID (without JSON quotes),
and start workers. None of these require an open UI or an open submission client:

```sh
python3 training/experiment_jobs.py --token-file .codex/run/e16/token submit \
  --spec .codex/run/e16/spec.json
python3 training/experiment_jobs.py --token-file .codex/run/e16/token worker \
  --evaluator target/debug/examples/e16_evaluate --name linux-a
# Start a second worker in another terminal, with --name linux-b.
python3 training/experiment_jobs.py --token-file .codex/run/e16/token status JOB_ID
python3 training/experiment_jobs.py --token-file .codex/run/e16/token cancel JOB_ID
python3 training/experiment_jobs.py --token-file .codex/run/e16/token resume JOB_ID \
  --evaluator target/debug/examples/e16_evaluate
```

`status` is plain structured JSON (no color or graphical-only information). It
shows total/finalized/failed seeds, evaluated scenario count, attempt failures,
partial bounded logs, current best reward, immutable result references, sorted
seed results, descriptive distribution, and usage. The default `random` method
samples enabled actions, including stop, with a deterministic xorshift stream
per seed. Set `algorithm` to `simulation` in a spec for repeated explicit-stop
baseline evaluations. Each episode reserves the same maximum three protocol
operations as E15 (reset and up to two steps); this is not an additional RL
method, a matched E15 algorithm comparison, or an optimizer/RNG checkpoint.

## Stored contract and restart (S01–S03)

Schema identifier: `e16-jobs-v1/e14-synthetic-v1`. Canonical sorted-key compact
JSON is hashed with SHA-256 for the job ID; duplicate submission is idempotent.
The immutable spec includes the **exact executable SHA-256**, synthetic topology
and demand-mix selectors, ordered unique seeds, algorithm, per-seed episode cap,
CPU/virtual-address/wall quotas, concurrency, retry budget and optional rate card.
The binary digest binds both compiled evaluator and embedded synthetic inputs;
there are no external datasets to hash in this version. A changed binary, fixture,
parameter or seed list creates a different job. Build separately on each target;
a job cannot mix executable digests across architectures/builds.

SQLite is coordinator-local, uses full synchronous transactions, and stores the
spec, item status, attempt history/logs/usage and accepted result payloads. Result
references `sqlite:JOB_ID/SEED` resolve through `status`, not a public file URL.
Keep the database on durable **local** storage with private permissions and normal
operator backups. Do not place it on NFS or expose SQLite directly to workers.
Reopen with the same schema and capacity; incompatible settings fail explicitly.
No migration from other schemas is implicit.

Cancellation is durable. The completion/cancellation transaction order wins:
results committed first remain finalized; later completions cannot publish after
stop. Workers target 200 ms heartbeat polling (a synchronous request can add up to
its five-second timeout), kill/reap the evaluator,
then acknowledge its resource usage. `stopping` does not become `cancelled` until
all outstanding process reservations have acknowledged reaping. A disconnected
worker stops on its next failed heartbeat; the Rust watchdog also bounds runtime
independently of the Python supervisor. Pending items remain restartable.

Resume requires a cancelled/failed job with all attempts accounted for, the same
binary digest and unchanged immutable spec. It retains completed seeds and resets
only unfinished items' retry budgets. Checkpoints are **seed boundaries**: an
interrupted seed restarts from its seed, not an arbitrary policy/training step.
The job need not retain a live coordinator or client while evaluation runs; loss
of the coordinator deliberately stops connected workers to avoid untracked spend.

## Isolation and quotas (S04)

One worker supervisor runs one repository-owned Rust evaluator at a time. The
child has no subprocesses and no domain parallelism; only a wall watchdog thread.
Before reading fixture data it installs and verifies Linux hard resource limits:

- `RLIMIT_CPU`: cumulative **CPU seconds per attempt**, not CPU shares/cores.
- `RLIMIT_AS`: **virtual address bytes**, not resident memory. Over-quota
  allocation is rejected by the kernel. Resident peak is separate telemetry.
- `RLIMIT_FSIZE`: 32 KiB output file cap; `RLIMIT_CORE`: zero.
- An evaluator-owned wall watchdog exits at the configured wall seconds, even if
  the Python supervisor disappears. The supervisor also kills the process group
  on cancellation/transport failure and always reaps before acknowledging.

Quota setup failure is terminal for that attempt and occurs before evaluation.
Do not substitute macOS RSS monitoring or claim it is an allocation hard limit:
native macOS workers are unsupported and fail closed. The Linux launch-path tests
exercise actual CPU kill, denied allocation, failed setup and independent orphan
wall cutoff, plus real cancellation and serial/parallel Rust evaluation.

Coordinator-wide capacity and per-job concurrency are **outstanding process
reservation** caps, enforced in the claim transaction. Lease expiry fences results
and schedules retry, but **does not release a reservation**: a partition or crashed
supervisor cannot establish that the child exited. A late worker may acknowledge
reaping and free its reservation without publishing a stale result. This is
conservative: a permanently lost worker can exhaust capacity and block work even
after its watchdog expired. There is intentionally no unsafe timer-based slot
reclamation or automatic operator "assume dead" command. Restore the worker to
acknowledge its saved completion, or reconcile the database offline only after
independently proving the old process is gone. Workers are trusted; this is not a
sandbox for arbitrary hostile executables, containers, fork bombs or tenant code.

## Remote workers, retry and fencing (S06)

The authenticated HTTP coordinator binds **only to 127.0.0.1**. Keep the token in a
private file, never in CLI arguments or logs. Workers on other machines use an
operator-managed SSH tunnel, for example:

```sh
ssh -N -L 8765:127.0.0.1:8765 COORDINATOR_HOST
# In another terminal on the worker, use the same client commands and token.
```

Do not expose the plaintext endpoint publicly; there is no multi-user authorization
or rate limiting. One token grants trusted operator access to every operation.
Use OS account boundaries, a private database directory, SSH authentication and
host-level resource budgets. Requests have bounded bodies and socket timeouts;
logs and scenario fields are validated. Errors are structured, and request logging
is disabled to avoid leaking bearer capabilities. Status hides attempt tokens.

A worker rejects unsupported platforms before requesting work and advertises its
verified binary digest; unmatched jobs consume neither reservations nor retries.
The executable is staged in a private directory, verified there and launched from
that immutable copy, so source rebuilds or symlink changes cannot swap checked
bytes for different launched bytes. Use one unique worker name per supervisor and
retain its private `.codex/run/e16-completions/NAME.json` recovery journal.

The worker persists a random request ID **before** claiming. Replaying that ID
recovers the same reservation after a lost reply. It journals the lease before
launch, launch intent before starting the child, and completion before publishing;
restarting a finished worker safely replays its identical completion without
requiring the old executable to remain installed. Recovery of an unlaunched claim
with missing/changed bytes acknowledges a known never-launched failure and releases
its reservation. A local journal lock prevents two supervisors from launching the
same recovered lease concurrently. A crash in
the launch-intent/active-process window deliberately fails closed on restart:
process exit must be independently established and reconciled offline. It cannot
safely infer that an evaluator did or did not start. A crash before launch or after
journaled completion is automatically recoverable. Do not discard recovery files
or run supervisors concurrently with the same name.

A claim transaction assigns an unpredictable per-attempt fencing capability and
fixed deadline (`wall_seconds + 10` seconds for setup/transport). Clocks must be
synchronized; workers reject delayed claims that cannot fit their entire wall
budget. Heartbeats do not extend reservations. An expired/stopped attempt cannot
publish. Accepted completion and the unique `(job, seed)` item result commit in
one transaction. Identical completion replays return the original outcome after a
lost reply; conflicting replays fail. Retries reproduce the seed from scratch.
Final results are sorted by seed, not arrival order. Failed/retried attempts remain
visible, including their usage; no failures are imputed as zero reward.

## Distribution, usage and estimates (S05/S07)

The aggregate includes accepted seed count, mean, sample variance (null for fewer
than two), minimum and maximum. Seeds vary optimizer action sampling, **not**
observed population uncertainty or simulator preference noise. These repository-
authored MIT synthetic fixtures are arbitrary modeled networks/demand/costs, not
observations. Distributions are descriptive, not confidence intervals. Best reward
is dimensionless cumulative E14 reward over completed episodes, maximized; the
same spec makes values comparable. Partial attempts can expose a provisional best
without becoming accepted seed results.

`usage` reports elapsed job wall seconds (including queue/pause time), summed
attempt wall/CPU seconds, maximum per-process resident bytes from Linux `wait4`,
unknown/unacknowledged attempts, and stored bytes. Stored bytes are the canonical
per-job JSON export size (`spec`, `attempts`, `results`), **not** SQLite page usage,
shared database overhead, local temporary logs or cloud-billing measurement.
Unknown attempts remain explicitly unknown; totals cover only acknowledged usage.

No rates are invented. With `rates: null`, estimated spend is null. Optional rates:

```json
{"currency":"USD","cpu_hour":0.04,"gib_hour":0.005,
 "storage_gib_month":0.02,"retention_hours":24}
```

These are user-supplied illustrative rates, not observed provider prices. Estimate
= CPU hours × CPU rate + reserved virtual GiB × attempt wall hours × memory rate
+ stored GiB × retention hours / 730 × storage rate. Unknown attempt usage makes
the estimate null. Currency, retention and units remain in the persisted spec.
CPU/virtual reservation pricing may differ materially from provider billing.

## Validation and performance

```sh
cargo build --locked --offline -p optimizer-bridge --examples
PYTHONPATH=training python3 -m unittest discover -s training/tests -p 'test_e16_*.py' -v
# Required on Linux; missing Linux/executable fails rather than silently skips:
PYTHONPATH=training python3 training/tests/e16_linux_acceptance.py
```

Control-plane tests map S01–S07 to durable restart, stop/resume, immutable digests,
capacity, parallel claims, late/replayed completion, HTTP authentication, aggregate
and rate calculations. Linux tests use the real Rust evaluator, never mock scores
for runtime quota evidence. Domain simulation algorithms remain unchanged.

For N seeds and A attempts, canonical submission and sorted result reporting are
O(N log N + A); claim/status scans are O(N + A), with bounded log bytes per attempt.
At most 102 bounded-width evaluator output lines (under 8 KiB) are emitted even
at the 1,000-episode cap, reserving ample space under the 32 KiB hard output limit.

Serialized SQLite writes favor correctness and small bounded reference runs, not
metro-scale throughput. Polling adds up to 200 ms progress latency and roughly
20 ms process-exit detection delay. A three-seed/eight-episode Linux debug reference run completed in 2.945 job wall
seconds (including worker startup), 0.364 summed attempt wall seconds and 0.295 CPU
seconds, with 26,210,304 bytes maximum resident peak. Worker containers used separate
process/network namespaces and authenticated HTTP through actual SSH tunnels to a
coordinator container. Lost claim/completion replies were injected and recovered by
restarting workers against their durable journals. All three seed results finalized
once. This is a single-VM multi-container demonstration, not separate physical-host
failure testing, a production deployment or a metropolitan scaling benchmark.
Results were descriptive synthetic reward (mean 1.63008); no spend was estimated
without supplied rates. No empirical calibration, car-independence conclusion or deployable
policy recommendation follows from completing a job.
