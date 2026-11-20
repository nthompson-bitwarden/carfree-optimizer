# E15 synthetic optimization benchmark

Run from the repository root (Python 3.11+, no added Python dependencies):

```sh
cargo build --locked --offline -p optimizer-bridge --examples
PYTHONPATH=training python3 -m unittest discover -s training/tests -p 'test_e15_*.py' -v
python3 training/optimizer_benchmark.py --server target/debug/examples/e15_batch \
  --output .codex/run/e15-demo
```

Open `index.html` in the new output directory. `report.json` records case IDs,
seeds, action traces, valid and rejected episodes, Pareto sets, and call counts.
`scratch.json` and `warm.json` are versioned inference checkpoints; `Policy.load`
restores a deterministic, masked policy (not optimizer/RNG resume state).

| Story | Benchmark contract |
| --- | --- |
| S01 | Uniform random over enabled actions at each decision; the same E14 Rust evaluator, opening budgets, stop action and hard guards as all other methods. |
| S02 | Greedy evaluates singleton portfolios before extending the strongest accepted singleton. Local search mutates the best accepted sequence. Both rank completed episodes by cumulative E14 reward and break score ties by lower capital spend. |
| S03 | Masked episodic Monte Carlo control trains on project-prefix states with epsilon-greedy exploration; frozen inference uses a stable stop-first, then index tie-break. Rejected episodes receive a training penalty but no benchmark score. |
| S04 | Evolutionary search mutates sampled Pareto parents. The archive maximizes cumulative objective and minimizes capital upper-bound spend in fixture currency major units; it is exact for sampled feasible episodes and their floating-point scores, not a global frontier. |
| S05 | Fixed synthetic topology, demand-mix and seed partitions. The Rust fixture changes the adult/mobility-aid expansion weights and matching modeled population without changing E14 evaluation or caps. Training runs before holdout evaluation, and evaluation never updates weights or settings. This is not an empirical geography split. |
| S06 | Matched per-case improvement versus explicit stop, completed-valid episode proportion, wall seconds, and sample variance across valid case rewards. Missing outcomes are not imputed to zero. |
| S07 | Same reserved training credits per arm; the warm arm replaces the first half of target training with source training. Both receive the same target adaptation schedule and deterministic held-out evaluation. Reports include paired warm-minus-scratch deltas and call counts. |

## Compute and interpretation

A credit is one Rust reset or attempted step, including explicit stops, failed
steps and prefix replays. All methods use the same total per-case cap (78 by
default). The RL-guided method is charged the full reserved scratch training
budget (48 credits by default) and corresponding training wall time for each
case, without amortization. Its first search episode is frozen inference; later
episodes may explore using the same fixed prior. Standalone frozen policies
are compared separately in the transfer ablation. The stop comparator and the
transfer diagnostics are not available to search and are outside its cap.
The evaluator reserves three credits before starting an episode, so it never
ranks a budget-cut partial trace. A rejected step is counted and invalidates that
episode; a transport failure aborts the run and produces no report. Process
startup and shutdown are included in measured wall time. Credits are a cap on
protocol operations, not a claim that operations take equal CPU time.

The harness does not run simulation in Python. For B call credits, trace
storage is O(B); sorting each sampled frontier and rescanning traces can take
O(B² log B) Python time over a run. The intended use is bounded reference
experiments, not high-throughput training. The included maps and demand are
repository-authored MIT synthetic fixtures, not observed populations or inputs
([E14 provenance and reward contract](RL-ENVIRONMENT.md)). The seed does not add
simulator preference noise in this fixture. Variance is descriptive, not a
confidence interval or evidence of empirical uncertainty.

On the default debug run (three held-out cases), random, greedy, local and
evolutionary search each averaged 1.599 objective gain versus stop; RL-guided
search averaged 1.528 under the matched total call cap. All five had 100%
validity in that run. Warm-start's paired mean delta was 0: **no demonstrated
transfer benefit**, and no evidence that RL outperforms simpler search. These
results do not establish simulator calibration, neighborhood car independence,
or suitability for deployment.
