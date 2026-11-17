# ADR 0001 — Rust-first simulator with a replaceable RL training backend

Status: Proposed

## Decision

Use Rust for graph, routing, scenario transformation, and simulation. Use Python/PyTorch initially for reinforcement learning through batched PyO3/maturin bindings. API will be Rust/Axum and visualization Svelte/MapLibre. Keep simulator independent of optimizer and frontend.

## Rationale

Rust offers strong control of memory layout, native parallel execution, and reproducible performance testing. PyTorch offers a mature ecosystem for RL and graph representation learning. A batched interface limits Python/Rust crossing costs.

## Alternatives

- Pure Rust training (Burn/Candle): fewer language boundaries but more implementation burden and less mature RL experimentation.
- Entirely Python simulation: rapid prototyping but potential throughput and memory penalties at city scale.
- External microsimulation for every RL step: useful fidelity but potentially prohibitive training costs.

## Verification needed

Benchmark with representative city data, assess Metropolis2/A-B Street reuse, measure FFI overhead, and compare predictions with a detailed simulator and observed data.
