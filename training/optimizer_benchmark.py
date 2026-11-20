"""E15 synthetic optimization benchmark. All simulation and scoring run in E14 Rust."""
from __future__ import annotations

import argparse
from contextlib import ExitStack
from dataclasses import asdict, dataclass
from html import escape
import json
import math
from pathlib import Path
import random
import statistics
from time import perf_counter

from carfree_env import BatchProcess, RejectedAction, Transition

# Fixed E14 v1 projection and ordered synthetic catalog. A change to either
# requires a new policy schema, even if dimensions are unchanged.
SCHEMA = "e15-v1/e14-observation-v1/e14-synthetic-actions-v1"
STOP = 3
CAPITAL = 17
WIDTH = 64
MAX_EPISODE_CALLS = 3  # one reset, at most two steps, including explicit stop
METHODS = ("random", "greedy", "local", "evolutionary", "rl-guided")


@dataclass(frozen=True)
class Scenario:
    topology: int
    demand: int
    seed: int


DEFAULT_TRAIN = (Scenario(0, 0, 7), Scenario(1, 1, 19))  # source, then target
DEFAULT_TEST = (Scenario(4, 3, 31), Scenario(7, 4, 47), Scenario(8, 3, 73))


@dataclass(frozen=True)
class Candidate:
    actions: tuple[int, ...]
    reward: float | None
    capital: float | None

    @property
    def valid(self):
        return self.reward is not None


def pareto(episodes):
    """Exact two-objective frontier of the sampled feasible episodes only."""
    ordered = sorted((e for e in episodes if e.valid),
                     key=lambda e: (e.capital, -e.reward, e.actions))
    front = []
    for e in ordered:
        if not front or e.reward > front[-1].reward:
            front.append(e)
    return front


def allowed(t):
    return [a for a, yes in enumerate(t.action_mask) if yes]


class Evaluator:
    def __init__(self, process, seed, budget):
        self.process, self.seed, self.budget = process, seed, budget
        self.resets = self.steps = 0

    def ready(self):
        return self.budget - self.resets - self.steps >= MAX_EPISODE_CALLS

    def episode(self, choose):
        if not self.ready():
            raise RuntimeError("episode exceeds reserved call budget")
        self.resets += 1
        t = self.process.reset([self.seed])[0]
        if t.terminated or t.truncated or t.reward != 0:
            raise RuntimeError("unexpected reset semantics")
        self.check_dimensions(t)
        capital = t.observation[CAPITAL]
        actions, trajectory = (), []
        reward = 0.0
        while len(actions) < MAX_EPISODE_CALLS - 1:
            if not allowed(t):
                raise RuntimeError("live state has no enabled action")
            action = choose(t, actions)
            if action not in allowed(t):
                raise RuntimeError("optimizer selected a masked action")
            trajectory.append((Policy.key(t, actions, action), reward))
            actions += (action,)
            self.steps += 1
            result = self.process.step([action])[0]
            if isinstance(result, RejectedAction):
                return Candidate(actions, None, None), trajectory
            t = result
            self.check_dimensions(t)
            reward += t.reward
            if t.terminated or t.truncated:
                return Candidate(actions, reward, capital - t.observation[CAPITAL]), trajectory
        raise RuntimeError("fixture exceeded declared horizon")

    @staticmethod
    def check_dimensions(t):
        if t.version != 1 or len(t.observation) != WIDTH or len(t.action_mask) != STOP + 1:
            raise RuntimeError("incompatible E15 fixture")

    def calls(self):
        return {"resets": self.resets, "steps": self.steps}


class Policy:
    """Masked first-visit Monte Carlo control with constant learning rate.

    Generalizes between the reference maps via project-prefix state abstraction;
    masks still use the full E14 state. This is not a general-purpose city encoder.
    """

    def __init__(self, values=None):
        self.values = {} if values is None else dict(values)

    @staticmethod
    def key(t: Transition, prefix, action):
        return (*prefix, action)

    def choose(self, t, prefix):
        return max(allowed(t), key=lambda a: (self.values.get(self.key(t, prefix, a), 0),
                                               a == STOP, -a))

    def learn(self, candidate, trajectory):
        for key, before in trajectory:
            # Invalid episodes are not ranked; teach the policy to avoid them.
            target = candidate.reward - before if candidate.valid else -1.0
            old = self.values.get(key, 0)
            self.values[key] = old + 0.5 * (target - old)

    def encode(self):
        return json.dumps({"schema": SCHEMA, "values": [[list(k), v] for k, v in sorted(self.values.items())]},
                          allow_nan=False, separators=(",", ":")) + "\n"

    def save(self, path):
        path.write_text(self.encode(), encoding="utf-8")

    @classmethod
    def load(cls, path):
        raw = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(raw, dict) or raw.get("schema") != SCHEMA or not isinstance(raw.get("values"), list):
            raise ValueError("incompatible checkpoint")
        values = {}
        for row in raw["values"]:
            if not isinstance(row, list) or len(row) != 2:
                raise ValueError("invalid checkpoint row")
            key, val = row
            if (not isinstance(key, list) or not 1 <= len(key) <= 2
                    or any(type(a) is not int or not 0 <= a <= STOP for a in key)
                    or (len(key) == 2 and key[0] == STOP) or tuple(key) in values
                    or type(val) not in (int, float) or not -1e6 <= val <= 1e6):
                raise ValueError("invalid checkpoint value")
            values[tuple(key)] = float(val)
        return cls(values)


@dataclass(frozen=True)
class Fit:
    scratch: Policy
    warm: Policy
    budget: int
    training_calls: dict
    training_seconds: dict


def connect(server, scenario):
    return BatchProcess([str(server), str(scenario.topology), str(scenario.demand)], 1)


def fit_pair(server, budget):
    """Equal reserved train credits; source replaces half of target training."""
    if budget < 6 or budget % 6:
        raise ValueError("train budget must be a positive multiple of six")
    source, target = DEFAULT_TRAIN
    phase_budget = budget // 2
    calls, seconds = {}, {}

    def phase(policy, scenario, rng_seed, name):
        rng = random.Random(rng_seed)
        start = perf_counter()
        with connect(server, scenario) as process:
            e = Evaluator(process, scenario.seed, phase_budget)
            for _ in range(phase_budget // MAX_EPISODE_CALLS):
                candidate, trace = e.episode(lambda t, p: rng.choice(allowed(t)) if rng.random() < 0.5
                                             else policy.choose(t, p))
                policy.learn(candidate, trace)
            calls[name] = e.calls()
        seconds[name] = perf_counter() - start

    scratch, warm = Policy(), Policy()
    phase(scratch, target, 101, "scratch-initial")
    phase(warm, source, 101, "warm-source")
    phase(scratch, target, 102, "scratch-target")
    phase(warm, target, 102, "warm-target")
    return Fit(scratch, warm, budget, calls, seconds)


def best(episodes):
    return max((e for e in episodes if e.valid), key=lambda e: (e.reward, -e.capital), default=None)


def proposal(method, records, rng, policy):
    if method == "random":
        return lambda t, _: rng.choice(allowed(t))
    if method == "rl-guided":
        return (lambda t, p: (rng.choice(allowed(t)) if records and rng.random() < 0.5
                              else policy.choose(t, p)))
    incumbent = best(records)
    if method == "greedy":
        # Evaluate singleton portfolios before extending the best singleton.
        singletons = [e for e in records if e.valid and len(e.actions) == 2 and e.actions[-1] == STOP]
        tried = {e.actions[0] for e in records}
        seed = best(singletons)

        def greedy(t, prefix):
            if not prefix:
                unexplored = [a for a in allowed(t) if a != STOP and a not in tried]
                if unexplored:
                    return unexplored[0]
                return seed.actions[0] if seed else STOP
            if prefix[0] not in tried:
                return STOP
            explored = {e.actions[1] for e in records if len(e.actions) == 2 and e.actions[:1] == prefix}
            return next((a for a in allowed(t) if a not in explored), STOP)

        return greedy
    if method == "evolutionary":
        front = pareto(records)
        incumbent = rng.choice(front) if front else None
    # Mutate a suffix of a best-so-far or Pareto parent. Recheck every mask.
    actions = incumbent.actions if incumbent else ()
    cut = rng.randrange(len(actions)) if actions else 0

    def mutate(t, prefix):
        i = len(prefix)
        choices = allowed(t)
        if i < cut and actions[i] in choices:
            return actions[i]
        if i == cut and actions and actions[i] in choices and len(choices) > 1:
            choices.remove(actions[i])
        return rng.choice(choices)

    return mutate


def record(e):
    return {**asdict(e), "valid": e.valid}


def benchmark(server, test, fitted, budget):
    if not test or len(set(test)) != len(test) or budget < fitted.budget + 2 * MAX_EPISODE_CALLS:
        raise ValueError("unique held-out cases and at least six credits beyond training required")
    for field in ("topology", "demand", "seed"):
        if {getattr(s, field) for s in test} & {getattr(s, field) for s in DEFAULT_TRAIN}:
            raise ValueError(f"train/holdout overlap: {field}")
    rows, pairs = [], []
    for s in test:
        with connect(server, s) as process:
            e = Evaluator(process, s.seed, 3 * MAX_EPISODE_CALLS)
            baseline, _ = e.episode(lambda *_: STOP)
            scratch, _ = e.episode(fitted.scratch.choose)
            warm, _ = e.episode(fitted.warm.choose)
        if not baseline.valid or baseline.reward != 0 or baseline.capital != 0:
            raise RuntimeError("invalid stop-only baseline")
        pairs.append({"scenario": asdict(s), "scratch": record(scratch), "warm": record(warm),
                      "delta": warm.reward - scratch.reward if warm.valid and scratch.valid else None})
        for method in METHODS:
            episodes = []
            rng = random.Random(s.seed)
            train_charge = fitted.budget if method == "rl-guided" else 0
            train_seconds = sum(v for k, v in fitted.training_seconds.items()
                                if k.startswith("scratch-")) if train_charge else 0.0
            start = perf_counter()
            with connect(server, s) as process:
                e = Evaluator(process, s.seed, budget - train_charge)
                while e.ready():
                    candidate, _ = e.episode(proposal(method, episodes, rng, fitted.scratch))
                    episodes.append(candidate)
            winner = best(episodes)
            rows.append({"method": method, "scenario": asdict(s), "budget": budget, "calls": e.calls(),
                         "training_charge": train_charge, "training_seconds_charged": train_seconds,
                         "baseline": baseline.reward, "best": winner.reward if winner else None,
                         "improvement": winner.reward - baseline.reward if winner else None,
                         "validity": sum(x.valid for x in episodes) / len(episodes),
                         "wall_seconds": perf_counter() - start, "episodes": [record(x) for x in episodes],
                         "pareto": [record(x) for x in pareto(episodes)] if method == "evolutionary" else []})
    summaries = []
    for method in METHODS:
        group = [r for r in rows if r["method"] == method]
        gains = [r["improvement"] for r in group if r["improvement"] is not None]
        summaries.append({"method": method, "valid_cases": len(gains),
                          "mean": statistics.mean(gains) if gains else None,
                          "variance": statistics.variance(gains) if len(gains) > 1 else None,
                          "validity": statistics.mean(r["validity"] for r in group),
                          "wall_seconds": sum(r["wall_seconds"] + r["training_seconds_charged"] for r in group)})
    valid_deltas = [r["delta"] for r in pairs if r["delta"] is not None]
    return {"schema": SCHEMA, "train": [asdict(s) for s in DEFAULT_TRAIN], "runs": rows, "summary": summaries,
            "ablation": {"reserved_credits_per_arm": fitted.budget,
                         "actual_training_calls": fitted.training_calls,
                         "training_wall_seconds": fitted.training_seconds, "pairs": pairs,
                         "mean_delta": statistics.mean(valid_deltas) if valid_deltas else None}}


def dashboard(report):
    def cell(value):
        return escape("—" if value is None else f"{value:.5g}" if isinstance(value, float) else str(value))

    def table(caption, headers, rows):
        return (f"<table><caption>{caption}</caption><thead><tr>" + "".join(f'<th scope="col">{h}</th>' for h in headers)
                + "</tr></thead><tbody>" + "".join("<tr>" + "".join(f"<td>{cell(x)}</td>" for x in row)
                                                  + "</tr>" for row in rows) + "</tbody></table>")

    summary = table("Held-out search summary", ("Method", "Valid cases", "Objective improvement (mean)",
                                               "Variance", "Validity rate", "Wall seconds"),
                    ((r[k] for k in ("method", "valid_cases", "mean", "variance", "validity", "wall_seconds"))
                     for r in report["summary"]))
    runs = table("Held-out search cases", ("Method", "Map", "Demand", "Seed", "Improvement", "Validity",
                                         "Total cap", "Train charge", "Search resets", "Search steps", "Wall seconds"),
                 ((r["method"], *r["scenario"].values(), r["improvement"], r["validity"], r["budget"],
                   r["training_charge"], *r["calls"].values(), r["wall_seconds"] + r["training_seconds_charged"])  for r in report["runs"]))
    ablation = report["ablation"]
    pairs = table("Frozen policy ablation", ("Map", "Demand", "Seed", "Scratch reward", "Warm reward", "Delta"),
                  ((*r["scenario"].values(), r["scratch"]["reward"], r["warm"]["reward"], r["delta"])
                   for r in ablation["pairs"]))
    return ("<!doctype html><html lang=en><meta charset=utf-8><title>E15 optimizer benchmark</title>"
            "<style>body{font:1rem system-ui;margin:2rem}table{border-collapse:collapse;margin:1rem 0}"
            "td,th{border:1px solid;padding:.4rem}caption{text-align:left;font-weight:bold}</style>"
            "<h1>E15 optimizer benchmark</h1><p>Synthetic only; not calibrated. Variance describes the "
            "listed cases, not empirical uncertainty. The RL-guided arm is charged its full reserved training "
            "budget and training wall time per case. Missing values are shown as —.</p>"
            + summary + runs + "<h2>Warm-start versus scratch</h2>"
            + f"<p>Reserved training calls per arm: {cell(ablation['reserved_credits_per_arm'])}. "
            f"Mean paired reward delta: {cell(ablation['mean_delta'])}.</p>" + pairs + "</html>\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="new report directory")
    parser.add_argument("--train-budget", type=int, default=48)
    parser.add_argument("--total-budget", type=int, default=78)
    args = parser.parse_args()
    fitted = fit_pair(args.server.resolve(), args.train_budget)
    report = benchmark(args.server.resolve(), DEFAULT_TEST, fitted, args.total_budget)
    args.output.mkdir()
    fitted.scratch.save(args.output / "scratch.json")
    fitted.warm.save(args.output / "warm.json")
    (args.output / "report.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    (args.output / "index.html").write_text(dashboard(report), encoding="utf-8")


if __name__ == "__main__":
    main()
