#!/usr/bin/env python3
"""Benchmark E16 serial versus two-immediately-queued analytical scans.

The harness drives the real DuckDB sidecar protocol, returns aggregate rows only,
and records no source rows or private project paths in its JSON report.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import subprocess
import tempfile
import threading
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ENGINE = ROOT / "target/release/tarik-engine-duckdb"
DEFAULT_REPORT = ROOT / "target/e16/scheduler-benchmark.json"
TERMINAL = {"succeeded", "failed", "cancelled"}


def directory_bytes(root: Path) -> int:
    if not root.exists():
        return 0
    total = 0
    for path in root.rglob("*"):
        try:
            if path.is_file() and not path.is_symlink():
                total += path.stat().st_size
        except FileNotFoundError:
            pass
    return total


def process_sample(pid: int) -> tuple[int, int, int]:
    rss_kib = hwm_kib = cpu_ticks = 0
    try:
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            key, _, value = line.partition(":")
            if key == "VmRSS":
                rss_kib = int(value.strip().split()[0])
            elif key == "VmHWM":
                hwm_kib = int(value.strip().split()[0])
        fields = Path(f"/proc/{pid}/stat").read_text().split()
        cpu_ticks = int(fields[13]) + int(fields[14])
    except (FileNotFoundError, IndexError, ValueError):
        pass
    return rss_kib, hwm_kib, cpu_ticks


@dataclass
class ProcessMetrics:
    peakRssKiB: int
    peakHwmKiB: int
    cpuMs: int
    peakCacheBytes: int


class Sampler:
    def __init__(self, pid: int, cache_root: Path):
        self.pid = pid
        self.cache_root = cache_root
        self.samples: list[tuple[int, int, int, int]] = []
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self._sample, name="e16-benchmark-sampler")

    def _sample(self) -> None:
        while not self.stop.is_set():
            rss, hwm, ticks = process_sample(self.pid)
            self.samples.append((rss, hwm, ticks, directory_bytes(self.cache_root)))
            self.stop.wait(0.01)

    def __enter__(self) -> "Sampler":
        self.thread.start()
        return self

    def __exit__(self, *_: object) -> None:
        self.stop.set()
        self.thread.join(timeout=2)
        self._sample()

    def metrics(self) -> ProcessMetrics:
        ticks_per_second = os.sysconf("SC_CLK_TCK") if hasattr(os, "sysconf") else 100
        first_ticks = self.samples[0][2] if self.samples else 0
        last_ticks = self.samples[-1][2] if self.samples else first_ticks
        return ProcessMetrics(
            peakRssKiB=max((sample[0] for sample in self.samples), default=0),
            peakHwmKiB=max((sample[1] for sample in self.samples), default=0),
            cpuMs=round((last_ticks - first_ticks) * 1000 / ticks_per_second),
            peakCacheBytes=max((sample[3] for sample in self.samples), default=0),
        )


class Engine:
    def __init__(self, executable: Path, cache_root: Path):
        self.cache_root = cache_root
        self.process = subprocess.Popen(
            [str(executable)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.sequence = 0
        self.request_latencies_ms: list[float] = []

    def request(self, method: str, params: dict[str, Any] | None = None) -> Any:
        self.sequence += 1
        request_id = f"e16-{self.sequence}"
        frame = {"id": request_id, "method": method, "params": params or {}}
        assert self.process.stdin and self.process.stdout
        started = time.perf_counter()
        self.process.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        while True:
            line = self.process.stdout.readline()
            if not line:
                stderr = self.process.stderr.read() if self.process.stderr else ""
                raise RuntimeError(f"engine exited during {method}: {stderr[-2000:]}")
            response = json.loads(line)
            if response.get("id") != request_id:
                continue
            self.request_latencies_ms.append((time.perf_counter() - started) * 1000)
            if not response.get("ok"):
                raise RuntimeError(f"{method}: {response.get('error')}")
            return response.get("result")

    def submit(
        self, execution_id: str, sql: str, session_id: str = "e16-benchmark"
    ) -> None:
        self.request(
            "query.execute",
            {
                "sessionId": session_id,
                "executionId": execution_id,
                "sql": sql,
                "cacheDir": str(self.cache_root),
                "rowLimit": 100,
                "maximumResultBytes": 8 * 1024 * 1024,
            },
        )

    def status(self, execution_id: str) -> dict[str, Any]:
        return self.request("query.status", {"executionId": execution_id})

    def release(self, execution_id: str) -> None:
        self.request("result.release", {"resultId": execution_id})

    def close(self) -> None:
        if self.process.poll() is None:
            try:
                self.request("engine.shutdown")
            except (BrokenPipeError, RuntimeError):
                pass
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, round((len(ordered) - 1) * fraction))
    return ordered[index]


def workload(
    engine: Engine,
    name: str,
    sql: str,
    mode: str,
) -> dict[str, Any]:
    ids = [f"{name}-a", f"{name}-b"]
    sessions = (
        ["e16-benchmark-a", "e16-benchmark-b"]
        if mode == "concurrent_pair"
        else ["e16-benchmark", "e16-benchmark"]
    )
    submitted_at: dict[str, float] = {}
    completed_at: dict[str, float] = {}
    status_latencies: list[float] = []
    states_seen: dict[str, list[str]] = {identifier: [] for identifier in ids}
    request_start = len(engine.request_latencies_ms)
    started = time.perf_counter()
    sampler = Sampler(engine.process.pid, engine.cache_root)
    with sampler:
        if mode in {"queued_pair", "concurrent_pair"}:
            for identifier, session_id in zip(ids, sessions, strict=True):
                submitted_at[identifier] = time.perf_counter()
                engine.submit(identifier, sql, session_id)
            remaining = set(ids)
            while remaining:
                for identifier in list(remaining):
                    before = time.perf_counter()
                    status = engine.status(identifier)
                    status_latencies.append((time.perf_counter() - before) * 1000)
                    state = status["state"]
                    if not states_seen[identifier] or states_seen[identifier][-1] != state:
                        states_seen[identifier].append(state)
                    if state in TERMINAL:
                        if state != "succeeded":
                            raise RuntimeError(f"{identifier} ended {state}: {status}")
                        completed_at[identifier] = time.perf_counter()
                        remaining.remove(identifier)
                if remaining:
                    time.sleep(0.005)
        else:
            for identifier, session_id in zip(ids, sessions, strict=True):
                submitted_at[identifier] = time.perf_counter()
                engine.submit(identifier, sql, session_id)
                while True:
                    before = time.perf_counter()
                    status = engine.status(identifier)
                    status_latencies.append((time.perf_counter() - before) * 1000)
                    state = status["state"]
                    if not states_seen[identifier] or states_seen[identifier][-1] != state:
                        states_seen[identifier].append(state)
                    if state in TERMINAL:
                        if state != "succeeded":
                            raise RuntimeError(f"{identifier} ended {state}: {status}")
                        completed_at[identifier] = time.perf_counter()
                        break
                    time.sleep(0.005)
    cache_before_release = directory_bytes(engine.cache_root)
    for identifier in ids:
        engine.release(identifier)
    cache_after_release = directory_bytes(engine.cache_root)
    latencies = [round((completed_at[value] - submitted_at[value]) * 1000) for value in ids]
    request_slice = engine.request_latencies_ms[request_start:]
    return {
        "mode": mode,
        "heavyExecutionPolicy": (
            "concurrent_benchmark_only" if mode == "concurrent_pair" else "serial"
        ),
        "totalDurationMs": round((time.perf_counter() - started) * 1000),
        "individualLatencyMs": latencies,
        "statusResponseP50Ms": round(statistics.median(status_latencies), 3),
        "statusResponseP95Ms": round(percentile(status_latencies, 0.95), 3),
        "allProtocolResponseP95Ms": round(percentile(request_slice, 0.95), 3),
        "statesSeen": states_seen,
        "cacheBytesBeforeRelease": cache_before_release,
        "cacheBytesAfterRelease": cache_after_release,
        "process": asdict(sampler.metrics()),
    }


def command_output(*command: str) -> str:
    try:
        return subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def run(engine_path: Path, rows: int) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="tarik-e16-benchmark-") as raw_root:
        root = Path(raw_root)
        database = root / "benchmark.duckdb"
        parquet = root / "benchmark.parquet"
        cache = root / "results"
        cache.mkdir()
        engine = Engine(engine_path, cache)
        try:
            handshake = engine.request("engine.handshake")
            sequential_resources = {
                "preset": "custom",
                "memoryLimitMib": 4096,
                "threads": 4,
            }
            concurrent_resources = {
                "preset": "custom",
                "memoryLimitMib": 4096,
                "threads": 2,
            }
            engine.request(
                "session.open",
                {
                    "sessionId": "e16-benchmark",
                    "locator": {"engineId": "duckdb", "payload": {"path": str(database)}},
                    "resources": sequential_resources,
                },
            )
            setup_id = "setup-dataset"
            engine.submit(
                setup_id,
                (
                    f"CREATE TABLE bench AS SELECT i::BIGINT AS id, (i % 10000)::INTEGER AS group_id, "
                    f"sin(i::DOUBLE) AS measure FROM range({rows}) t(i); "
                    f"COPY bench TO '{parquet.as_posix()}' (FORMAT PARQUET, COMPRESSION ZSTD)"
                ),
            )
            while True:
                setup = engine.status(setup_id)
                if setup["state"] in TERMINAL:
                    if setup["state"] != "succeeded":
                        raise RuntimeError(f"dataset setup failed: {setup}")
                    break
                time.sleep(0.01)

            table_sql = "SELECT group_id % 97 AS bucket, sum(sqrt(abs(measure)) + id % 17) AS score FROM bench GROUP BY 1 ORDER BY 1"
            parquet_sql = f"SELECT group_id % 97 AS bucket, sum(sqrt(abs(measure)) + id % 17) AS score FROM read_parquet('{parquet.as_posix()}') GROUP BY 1 ORDER BY 1"
            engine.request(
                "session.open",
                {
                    "sessionId": "e16-benchmark-a",
                    "locator": {"engineId": "duckdb", "payload": {"path": str(database)}},
                    "resources": concurrent_resources,
                },
            )
            engine.request(
                "session.open",
                {
                    "sessionId": "e16-benchmark-b",
                    "locator": {"engineId": "duckdb", "payload": {"path": str(database)}},
                    "resources": concurrent_resources,
                },
            )
            scenarios = {
                "physicalTable": {
                    "sequential": workload(
                        engine, "table-sequential", table_sql, "sequential"
                    ),
                    "queuedPair": workload(
                        engine, "table-queued", table_sql, "queued_pair"
                    ),
                    "concurrentPair": workload(
                        engine, "table-concurrent", table_sql, "concurrent_pair"
                    ),
                },
                "parquet": {
                    "sequential": workload(
                        engine, "parquet-sequential", parquet_sql, "sequential"
                    ),
                    "queuedPair": workload(
                        engine, "parquet-queued", parquet_sql, "queued_pair"
                    ),
                    "concurrentPair": workload(
                        engine, "parquet-concurrent", parquet_sql, "concurrent_pair"
                    ),
                },
            }
            residue = directory_bytes(cache)
            return {
                "schemaVersion": 1,
                "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "machine": {
                    "os": platform.platform(),
                    "architecture": platform.machine(),
                    "logicalCpuCount": os.cpu_count(),
                    "rustc": command_output("rustc", "--version"),
                    "gitRevision": command_output("git", "-C", str(ROOT), "rev-parse", "HEAD"),
                    "engine": str(engine_path.relative_to(ROOT) if engine_path.is_relative_to(ROOT) else engine_path),
                },
                "engine": handshake,
                "resources": {
                    "sequential": sequential_resources,
                    "concurrentPerQuery": concurrent_resources,
                    "concurrentAggregateThreads": 4,
                    "memoryLimitScope": "shared engine process",
                },
                "dataset": {
                    "rows": rows,
                    "physicalDatabaseBytes": database.stat().st_size,
                    "parquetBytes": parquet.stat().st_size,
                    "returnedRowsPerQuery": 97,
                },
                "scenarios": scenarios,
                "resultCacheResidueBytes": residue,
                "verdict": "retain_serial_heavy_execution",
                "limitations": [
                    "Linux sidecar benchmark; no WebView2 or native Windows process evidence.",
                    "True concurrent pairs run only in isolated benchmark sessions; production remains serial.",
                    "Peak RSS/HWM and CPU are process-level observations, not per-query attribution.",
                ],
            }
        finally:
            engine.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--rows", type=int, default=2_000_000)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    args = parser.parse_args()
    if args.rows < 100_000:
        parser.error("--rows must be at least 100000")
    if not args.engine.is_file():
        parser.error(f"engine not found: {args.engine}")
    report = run(args.engine.resolve(), args.rows)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({
        "report": str(args.report),
        "rows": report["dataset"]["rows"],
        "verdict": report["verdict"],
        "cacheResidueBytes": report["resultCacheResidueBytes"],
    }))


if __name__ == "__main__":
    main()
