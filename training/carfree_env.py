"""E14 persistent, bounded Rust batch binding; no simulation in Python.

POSIX pipe transport. A process is private to one caller; calls are serialized.
Malformed responses/timeouts invalidate the connection, never trigger a retry of
an uncertain transition. See docs/RL-ENVIRONMENT.md for the versioned wire schema.
"""
from __future__ import annotations

from dataclasses import dataclass
import math
import os
import selectors
import subprocess
import time
from typing import Sequence

MAX_SLOTS = 64
MAX_INPUT_BYTES = 65_536
MAX_FEATURES = 4096
MAX_ACTIONS = 1024
MAX_OUTPUT_BYTES = MAX_SLOTS * (MAX_FEATURES * 32 + MAX_ACTIONS + 256)


class ProtocolError(RuntimeError):
    """The connection is unusable; restart/reset, do not retry a step."""


class RejectedAction(ValueError):
    """The Rust environment explicitly rejected an operation without mutation."""


@dataclass(frozen=True)
class Transition:
    version: int
    state: int
    observation: tuple[float, ...]
    action_mask: tuple[bool, ...]
    reward: float
    terminated: bool
    truncated: bool


def integer(value: object, name: str, maximum: int = 2**64 - 1) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value <= maximum:
        raise ValueError(f"{name} must be an integer in [0, {maximum}]")
    return value


class BatchProcess:
    """One persistent Rust process, bounded slots and one operation per slot/batch.

    The executable must create exactly ``slots`` environments. Subset requests
    retain caller order. Runtime errors are per-slot, not a batch rollback.
    """

    def __init__(self, command: Sequence[str], slots: int, timeout: float = 30.0):
        if os.name != "posix":
            raise OSError("the E14 process binding currently requires POSIX pipes")
        self.slots = integer(slots, "slots", MAX_SLOTS)
        if not self.slots or not command or isinstance(command, (str, bytes)):
            raise ValueError("nonempty argv and 1..64 slots required")
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("timeout must be positive and finite")
        self.timeout = timeout
        self._states: dict[int, Transition] = {}
        self._buffer = bytearray()
        self._closed = False
        self._process = subprocess.Popen(
            list(command), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=None, bufsize=0,
        )
        self._selector = selectors.DefaultSelector()
        assert self._process.stdout is not None
        self._selector.register(self._process.stdout, selectors.EVENT_READ)
        assert self._process.stdin is not None
        os.set_blocking(self._process.stdin.fileno(), False)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        if self._closed:
            return
        self._closed = True
        self._selector.close()
        if self._process.stdin:
            self._process.stdin.close()
        if self._process.poll() is None:
            self._process.terminate()
            try:
                self._process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self._process.kill()
                self._process.wait()
        if self._process.stdout:
            self._process.stdout.close()

    def _line(self, deadline: float, remaining: int) -> bytes:
        scan_from = 0
        while True:
            newline = self._buffer.find(b"\n", scan_from)
            if newline >= 0:
                count = newline + 1
                if count > remaining:
                    raise ProtocolError("response exceeds byte limit")
                row = bytes(self._buffer[:count])
                del self._buffer[:count]
                return row
            left = deadline - time.monotonic()
            if left <= 0 or not self._selector.select(left):
                raise ProtocolError("Rust response timed out; transition outcome uncertain")
            assert self._process.stdout is not None
            chunk = os.read(self._process.stdout.fileno(), min(8192, remaining + 1))
            if not chunk:
                raise ProtocolError("Rust process closed its response stream")
            scan_from = len(self._buffer)
            self._buffer.extend(chunk)
            if len(self._buffer) > remaining:
                raise ProtocolError("response exceeds byte limit")

    def request(self, operations: Sequence[tuple]) -> list[Transition | RejectedAction]:
        """Operations: ('reset', slot, seed), ('step', slot, state, action)."""
        if self._closed:
            raise ProtocolError("connection closed")
        if not 1 <= len(operations) <= MAX_SLOTS:
            raise ValueError("batch must have 1..64 operations")
        seen = set()
        rows = [f"E14\t1\t{len(operations)}\n"]
        for op in operations:
            if not isinstance(op, tuple) or len(op) < 2:
                raise ValueError("invalid operation")
            slot = integer(op[1], "slot", self.slots - 1)
            if slot in seen:
                raise ValueError("duplicate batch slot")
            seen.add(slot)
            if op[0] == "reset" and len(op) == 3:
                rows.append(f"reset\t{slot}\t{integer(op[2], 'seed')}\n")
            elif op[0] == "step" and len(op) == 4:
                rows.append(f"step\t{slot}\t{integer(op[2], 'state')}\t{integer(op[3], 'action', 2**32 - 1)}\n")
            else:
                raise ValueError("invalid operation/field count")
        frame = "".join(rows).encode("ascii")
        if len(frame) > MAX_INPUT_BYTES:
            raise ValueError("request exceeds byte limit")
        try:
            assert self._process.stdin is not None
            deadline = time.monotonic() + self.timeout
            with selectors.DefaultSelector() as writable:
                writable.register(self._process.stdin, selectors.EVENT_WRITE)
                sent = 0
                while sent < len(frame):
                    left = deadline - time.monotonic()
                    if left <= 0 or not writable.select(left):
                        raise ProtocolError("Rust request timed out; transition outcome uncertain")
                    try:
                        sent += os.write(self._process.stdin.fileno(), frame[sent:])
                    except BlockingIOError:
                        continue
            remaining = MAX_OUTPUT_BYTES
            raw = self._line(deadline, remaining)
            remaining -= len(raw)
            if raw != f"E14\t1\t{len(operations)}\n".encode():
                raise ProtocolError("unsupported response header/count")
            results: list[Transition | RejectedAction] = []
            for op in operations:
                raw = self._line(deadline, remaining)
                remaining -= len(raw)
                fields = raw[:-1].decode("ascii").split("\t")
                slot = int(fields[1])
                if slot != op[1]:
                    raise ProtocolError("response slot order mismatch")
                if (fields[0] == "error" and len(fields) == 3 and fields[2]
                        and len(fields[2]) <= 512 and all(32 <= ord(c) < 127 for c in fields[2])):
                    results.append(RejectedAction(fields[2]))
                    continue
                if len(fields) != 9 or fields[0] != "ok" or fields[2] != "1":
                    raise ProtocolError("invalid transition schema")
                if fields[5] not in ("0", "1") or fields[6] not in ("0", "1"):
                    raise ProtocolError("invalid terminal flags")
                if not fields[8] or len(fields[8]) > MAX_ACTIONS or set(fields[8]) - {"0", "1"}:
                    raise ProtocolError("invalid action mask")
                observation = tuple(map(float, fields[7].split(","))) if fields[7] else ()
                reward = float(fields[4])
                if len(observation) > MAX_FEATURES or not all(map(math.isfinite, (*observation, reward))):
                    raise ProtocolError("invalid observation/reward")
                state = integer(int(fields[3]), "response state")
                t = Transition(1, state, observation, tuple(c == "1" for c in fields[8]), reward, fields[5] == "1", fields[6] == "1")
                if t.terminated and t.truncated:
                    raise ProtocolError("both terminal flags set")
                previous = self._states.get(slot)
                if previous and (len(previous.observation) != len(t.observation) or len(previous.action_mask) != len(t.action_mask)):
                    raise ProtocolError("observation/action dimensions changed")
                if previous and t.state <= previous.state:
                    raise ProtocolError("state revision did not advance")
                results.append(t)
            # A persistent server owes exactly one frame per request. Reject
            # unsolicited buffered/ready bytes before publishing any local state.
            if self._buffer:
                raise ProtocolError("unsolicited trailing response data")
            if self._selector.select(0):
                assert self._process.stdout is not None
                if os.read(self._process.stdout.fileno(), 8192):
                    raise ProtocolError("unsolicited trailing response data")
            # Publish local state only after validating the complete response.
            for op, result in zip(operations, results):
                if isinstance(result, Transition):
                    self._states[op[1]] = result
            return results
        except (OSError, ValueError, IndexError, ProtocolError) as error:
            self.close()
            raise ProtocolError(f"Rust batch failed: {error}") from error
        except BaseException:
            # Interrupts after write may leave an executed-but-unread transition.
            # Preserve cancellation semantics, but never allow connection reuse.
            self.close()
            raise

    def reset(self, seeds: Sequence[int]) -> list[Transition]:
        if len(seeds) != self.slots:
            raise ValueError("one seed per slot required")
        results = self.request([("reset", slot, seed) for slot, seed in enumerate(seeds)])
        for result in results:
            if isinstance(result, RejectedAction):
                raise result
        return results  # type: ignore[return-value]

    def step(self, actions: Sequence[int]) -> list[Transition | RejectedAction]:
        if len(actions) != self.slots or len(self._states) != self.slots:
            raise ValueError("reset every slot before step; one action per slot required")
        return self.request([("step", slot, self._states[slot].state, action) for slot, action in enumerate(actions)])


# Training-only dependencies; the bounded transport itself uses the stdlib.
try:
    import gymnasium as gym
    import numpy as np
except ImportError:
    gym = None


if gym is not None:
    class GymEnvironment(gym.Env):
        """Gymnasium v1 reset/step over one Rust slot, with mask in info."""
        metadata = {"render_modes": []}

        def __init__(self, command: Sequence[str], timeout: float = 30.0):
            self.batch = BatchProcess(command, 1, timeout)
            try:
                initial = self.batch.reset([0])[0]
                self.observation_space = gym.spaces.Box(-np.inf, np.inf, (len(initial.observation),), dtype=np.float64)
                self.action_space = gym.spaces.Discrete(len(initial.action_mask))
            except BaseException:
                self.batch.close()
                raise

        @staticmethod
        def _info(t: Transition):
            return {"schema_version": t.version,
                    "action_mask": np.asarray(t.action_mask, dtype=np.bool_)}

        def reset(self, *, seed=None, options=None):
            super().reset(seed=seed)
            if options:
                raise ValueError("no reset options supported")
            actual_seed = int(self.np_random.integers(0, 2**63)) if seed is None else integer(seed, "seed")
            t = self.batch.reset([actual_seed])[0]
            return np.asarray(t.observation, dtype=np.float64), self._info(t)

        def step(self, action):
            if not self.action_space.contains(action):
                raise RejectedAction("action outside catalog")
            result = self.batch.step([int(action)])[0]
            if isinstance(result, RejectedAction):
                raise result
            return (np.asarray(result.observation, dtype=np.float64), result.reward,
                    result.terminated, result.truncated, self._info(result))

        def close(self):
            self.batch.close()
else:
    class GymEnvironment:
        def __init__(self, *_args, **_kwargs):
            raise ImportError("Install training/requirements.txt for the Gymnasium adapter")
