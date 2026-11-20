# Reference environment training boundary (E14)

`carfree_env.py` provides a bounded persistent Rust-process binding and an actual
Gymnasium adapter. It does not simulate trips or calculate rewards in Python.
Gymnasium and NumPy are training-only dependencies pinned in `requirements.txt`.
See [the full contract and setup](../docs/RL-ENVIRONMENT.md).

```sh
python3 -m venv .codex/run/e14-venv
.codex/run/e14-venv/bin/python -m pip install -r training/requirements.txt
cargo build --locked --offline -p optimizer-bridge --examples
PYTHONPATH=training .codex/run/e14-venv/bin/python -m unittest discover -s training/tests -v
cargo build --release --locked --offline -p optimizer-bridge --examples
PYTHONPATH=training .codex/run/e14-venv/bin/python training/profile_e14.py --size 1
PYTHONPATH=training .codex/run/e14-venv/bin/python training/profile_e14.py --size 8
cargo run --release --locked --offline -p optimizer-bridge --example e14_experiments
```

The bundled server is a deterministic **synthetic reference fixture**, not a city
loader. Rust transfer experiments compare equally budgeted fitted vector-linear
and graph-message-linear policies on unseen synthetic topologies, with frozen
training-only scaling, reward ablations, decompositions, failure counts and a
stop-only baseline. They are not PyTorch/GNN/RL training or empirical cross-city
validation. No graph superiority, calibrated car independence or deployment
readiness is assumed.

[E15 optimization benchmarks](../docs/OPTIMIZATION.md) compare random, greedy,
local, evolutionary and RL-guided search on matched synthetic cases, and
include a deterministic warm-start ablation and a standalone HTML dashboard.
PyO3/maturin and PyTorch remain future, independently justified integrations.
