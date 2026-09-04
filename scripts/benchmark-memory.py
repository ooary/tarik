#!/usr/bin/env python3
"""Repeatable Linux memory/disk regression harness for Tarik's DuckDB sidecar.

The harness never stores query rows in its report. It drives the real newline-JSON
protocol, samples /proc RSS/HWM, verifies result/stage cleanup, and compares the
run with version-controlled conservative budgets.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import tempfile
import threading
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ENGINE = ROOT / "target/release/tarik-engine-duckdb"
DEFAULT_BUDGETS = ROOT / "docs/performance/E11-MEMORY-BUDGETS.json"
DEFAULT_REPORT = ROOT / "target/e11/memory-report.json"
TERMINAL = {"succeeded", "failed", "cancelled"}


@dataclass
class Sample:
    elapsedMs: int
    rssKiB: int
    hwmKiB: int
    cacheBytes: int
    outputBytes: int


class Engine:
    def __init__(self, executable: Path, result_root: Path, output_root: Path):
        self.result_root = result_root
        self.output_root = output_root
        self.proc = subprocess.Popen(
            [str(executable)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        assert self.proc.stdin and self.proc.stdout
        self._sequence = 0

    def request(self, method: str, params: dict[str, Any] | None = None) -> Any:
        self._sequence += 1
        request_id = f"bench-{self._sequence}"
        frame = {"id": request_id, "method": method, "params": params or {}}
        assert self.proc.stdin and self.proc.stdout
        self.proc.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.proc.stdin.flush()
        while True:
            line = self.proc.stdout.readline()
            if not line:
                stderr = self.proc.stderr.read() if self.proc.stderr else ""
                raise RuntimeError(f"engine exited before {method}: {stderr[-2000:]}")
            response = json.loads(line)
            if response.get("id") != request_id:
                continue
            if not response.get("ok"):
                raise RuntimeError(f"{method}: {response.get('error')}")
            return response.get("result")

    def poll(self, kind: str, identifier: str, timeout: float = 120.0) -> dict[str, Any]:
        method = f"{kind}.status"
        key = "executionId" if kind == "query" else "exportId"
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            status = self.request(method, {key: identifier})
            if status["state"] in TERMINAL:
                return status
            time.sleep(0.02)
        raise TimeoutError(f"{kind} {identifier} exceeded {timeout}s")

    def execute(self, session: str, execution_id: str, sql: str) -> dict[str, Any]:
        self.request(
            "query.execute",
            {
                "sessionId": session,
                "executionId": execution_id,
                "sql": sql,
                "cacheDir": str(self.result_root),
            },
        )
        status = self.poll("query", execution_id)
        if status["state"] != "succeeded":
            raise RuntimeError(f"query {execution_id}: {status}")
        return status

    def close(self) -> None:
        if self.proc.poll() is None:
            try:
                self.request("engine.shutdown")
            except (BrokenPipeError, RuntimeError):
                pass
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=5)


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


def proc_memory(pid: int) -> tuple[int, int]:
    values = {"VmRSS": 0, "VmHWM": 0}
    try:
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            key, _, rest = line.partition(":")
            if key in values:
                values[key] = int(rest.strip().split()[0])
    except FileNotFoundError:
        pass
    return values["VmRSS"], values["VmHWM"]


class Sampler:
    def __init__(self, engine: Engine):
        self.engine = engine
        self.started = time.monotonic()
        self.samples: list[Sample] = []
        self.stop_event = threading.Event()
        self.thread = threading.Thread(target=self._run, name="tarik-memory-sampler")

    def _run(self) -> None:
        while not self.stop_event.is_set():
            rss, hwm = proc_memory(self.engine.proc.pid)
            self.samples.append(
                Sample(
                    elapsedMs=round((time.monotonic() - self.started) * 1000),
                    rssKiB=rss,
                    hwmKiB=hwm,
                    cacheBytes=directory_bytes(self.engine.result_root),
                    outputBytes=directory_bytes(self.engine.output_root),
                )
            )
            self.stop_event.wait(0.02)

    def __enter__(self) -> "Sampler":
        self.thread.start()
        return self

    def __exit__(self, *_: object) -> None:
        self.stop_event.set()
        self.thread.join(timeout=2)
        self._run()


def write_csv(path: Path, rows: int) -> None:
    with path.open("w", encoding="utf-8", newline="") as output:
        output.write("id,market_id,amount\n")
        for index in range(rows):
            output.write(f"{index},{index % 100},{(index % 1000) / 10:.1f}\n")


def terminal_count(root: Path) -> int:
    return sum(1 for path in root.iterdir()) if root.exists() else 0


def machine_metadata(engine: Path) -> dict[str, Any]:
    def output(*command: str) -> str:
        try:
            return subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()
        except (OSError, subprocess.CalledProcessError):
            return "unavailable"

    mem_total = "unavailable"
    try:
        mem_total = next(
            line.split(":", 1)[1].strip()
            for line in Path("/proc/meminfo").read_text().splitlines()
            if line.startswith("MemTotal:")
        )
    except (OSError, StopIteration):
        pass
    cpu = "unavailable"
    try:
        cpu = next(
            line.split(":", 1)[1].strip()
            for line in Path("/proc/cpuinfo").read_text().splitlines()
            if line.startswith("model name")
        )
    except (OSError, StopIteration):
        pass
    return {
        "os": platform.platform(),
        "architecture": platform.machine(),
        "cpu": cpu,
        "logicalCpuCount": os.cpu_count(),
        "memoryTotal": mem_total,
        "python": platform.python_version(),
        "rustc": output("rustc", "--version"),
        "node": output("node", "--version"),
        "engine": str(engine.relative_to(ROOT) if engine.is_relative_to(ROOT) else engine),
        "gitRevision": output("git", "-C", str(ROOT), "rev-parse", "HEAD"),
    }


def run(engine_path: Path) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="tarik-memory-") as raw_root:
        root = Path(raw_root)
        result_root = root / "results"
        output_root = root / "exports"
        result_root.mkdir()
        output_root.mkdir()
        database = root / "benchmark.duckdb"
        csv = root / "orders.csv"
        dataset = {
            "importRows": 50_000,
            "largeResultRows": 100_000,
            "largeResultPayloadBytes": 128,
            "repeatCycles": 12,
            "repeatRows": 20_000,
            "exportRowsRequested": 1_000_000_000,
            "exportPayloadBytes": 512,
            "exportRowsPerPart": 100_000,
        }
        write_csv(csv, dataset["importRows"])
        engine = Engine(engine_path, result_root, output_root)
        scenarios: dict[str, dict[str, Any]] = {}
        try:
            with Sampler(engine) as sampler:
                info = engine.request("engine.handshake")
                engine.request(
                    "session.open",
                    {
                        "sessionId": "bench",
                        "locator": {
                            "engineId": "duckdb",
                            "payload": {"path": str(database)},
                        },
                    },
                )
                time.sleep(0.15)
                idle_rss, idle_hwm = proc_memory(engine.proc.pid)
                scenarios["idle"] = {"rssKiB": idle_rss, "hwmKiB": idle_hwm}

                start = time.monotonic()
                inspected = engine.request("source.inspect", {"path": str(csv), "csv": None})
                imported = engine.request(
                    "duckdb.source.import_table",
                    {
                        "sessionId": "bench",
                        "projectId": "benchmark",
                        "path": str(csv),
                        "options": {
                            "tableName": "orders",
                            "csv": {
                                "delimiter": ",",
                                "hasHeader": True,
                                "nullValue": None,
                                "allVarchar": False,
                            },
                            "columnOverrides": [],
                        },
                    },
                )
                scenarios["import"] = {
                    "durationMs": round((time.monotonic() - start) * 1000),
                    "inspectedRows": inspected["rowCount"],
                    "importedRows": imported["options"]["rowCount"],
                }

                start = time.monotonic()
                large = engine.execute(
                    "bench",
                    "large-result",
                    "SELECT i, repeat('x', 128) AS payload FROM range(100000) t(i)",
                )
                first_page = engine.request(
                    "result.get_page",
                    {"resultId": "large-result", "offset": 0, "maxRows": 500},
                )
                last_page = engine.request(
                    "result.get_page",
                    {"resultId": "large-result", "offset": 99500, "maxRows": 500},
                )
                cache_before_release = directory_bytes(result_root)
                engine.request("result.release", {"resultId": "large-result"})
                scenarios["largeResult"] = {
                    "durationMs": round((time.monotonic() - start) * 1000),
                    "rows": large["rowsProduced"],
                    "firstPageRows": len(first_page["rows"]),
                    "lastPageRows": len(last_page["rows"]),
                    "cacheBytesBeforeRelease": cache_before_release,
                    "cacheBytesAfterRelease": directory_bytes(result_root),
                }

                rss_before_cycles, _ = proc_memory(engine.proc.pid)
                start = time.monotonic()
                for cycle in range(dataset["repeatCycles"]):
                    execution_id = f"cycle-{cycle}"
                    engine.execute(
                        "bench",
                        execution_id,
                        "SELECT i, i * 2 AS doubled FROM range(20000) t(i)",
                    )
                    engine.request(
                        "result.get_page",
                        {"resultId": execution_id, "offset": 19500, "maxRows": 500},
                    )
                    engine.request("result.release", {"resultId": execution_id})
                time.sleep(0.15)
                rss_after_cycles, _ = proc_memory(engine.proc.pid)
                scenarios["repeatedQuery"] = {
                    "durationMs": round((time.monotonic() - start) * 1000),
                    "cycles": dataset["repeatCycles"],
                    "rssBeforeKiB": rss_before_cycles,
                    "rssAfterKiB": rss_after_cycles,
                    "growthKiB": max(0, rss_after_cycles - rss_before_cycles),
                    "cacheBytesAfterRelease": directory_bytes(result_root),
                }

                engine.request(
                    "export.execute",
                    {
                        "sessionId": "bench",
                        "exportId": "cancel-export",
                        "sql": "SELECT i, repeat('x', 512) AS payload FROM range(1000000000) t(i)",
                        "options": {
                            "format": "csv",
                            "outputDirectory": str(output_root),
                            "baseName": "cancelled",
                            "rowsPerPart": 100000,
                            "overwrite": "fail_if_exists",
                            "csv": {"delimiter": ",", "includeHeader": True},
                            "parquet": None,
                        },
                    },
                )
                deadline = time.monotonic() + 10
                running = None
                while time.monotonic() < deadline:
                    running = engine.request("export.status", {"exportId": "cancel-export"})
                    if running["state"] == "running":
                        break
                    time.sleep(0.01)
                time.sleep(0.08)
                engine.request("export.cancel", {"exportId": "cancel-export"})
                cancelled = engine.poll("export", "cancel-export")
                stages = [
                    path.name
                    for path in output_root.iterdir()
                    if path.name.startswith(".tarik-export-")
                ]
                scenarios["exportCancel"] = {
                    "state": cancelled["state"],
                    "publishedRows": cancelled["rowsWritten"],
                    "publishedFiles": cancelled["filesWritten"],
                    "hiddenStagesAfterCancel": stages,
                    "outputBytes": directory_bytes(output_root),
                }
                engine.request("result.release_all")
                engine.request("session.close", {"sessionId": "bench"})

            samples = [asdict(sample) for sample in sampler.samples]
            peak_rss = max(sample["rssKiB"] for sample in samples)
            peak_hwm = max(sample["hwmKiB"] for sample in samples)
            report = {
                "schemaVersion": 1,
                "recordedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "machine": machine_metadata(engine_path),
                "engineInfo": info,
                "dataset": dataset,
                "defaults": {
                    "enginePageRows": 500,
                    "enginePageByteTarget": 4 * 1024 * 1024,
                    "desktopDecodedPages": 12,
                    "sessionQueryWorkers": 1,
                    "sessionExportWorkers": 1,
                    "parquetRowGroupByteTarget": 4 * 1024 * 1024,
                },
                "scenarios": scenarios,
                "summary": {
                    "peakSidecarRssKiB": peak_rss,
                    "peakSidecarHwmKiB": peak_hwm,
                    "postCycleGrowthKiB": scenarios["repeatedQuery"]["growthKiB"],
                    "resultCacheBytesAfterRelease": directory_bytes(result_root),
                    "hiddenExportStagesAfterCancel": len(
                        scenarios["exportCancel"]["hiddenStagesAfterCancel"]
                    ),
                },
                "samples": samples,
            }
            return report
        finally:
            engine.close()


def check(report: dict[str, Any], budgets: dict[str, Any]) -> list[str]:
    summary = report["summary"]
    failures = []
    checks = {
        "peakSidecarRssKiB": summary["peakSidecarRssKiB"],
        "postCycleGrowthKiB": summary["postCycleGrowthKiB"],
        "resultCacheBytesAfterRelease": summary["resultCacheBytesAfterRelease"],
        "hiddenExportStagesAfterCancel": summary["hiddenExportStagesAfterCancel"],
    }
    for name, actual in checks.items():
        maximum = budgets["maximums"][name]
        if actual > maximum:
            failures.append(f"{name}: {actual} exceeds {maximum}")
    if report["scenarios"]["exportCancel"]["state"] != "cancelled":
        failures.append("export cancellation did not reach cancelled")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--budgets", type=Path, default=DEFAULT_BUDGETS)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--record", action="store_true", help="also update docs/performance/latest-memory-report.json")
    args = parser.parse_args()
    if platform.system() != "Linux" or not Path("/proc/self/status").is_file():
        parser.error("memory benchmarking currently requires Linux /proc")
    if not args.engine.is_file():
        parser.error(f"release engine missing: {args.engine}; run CARGO_BUILD_PROFILE=release ./scripts/build-engine.sh")
    budgets = json.loads(args.budgets.read_text())
    report = run(args.engine.resolve())
    failures = check(report, budgets)
    report["budget"] = {
        "source": str(args.budgets.relative_to(ROOT)),
        "passed": not failures,
        "failures": failures,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    if args.record:
        recorded = ROOT / "docs/performance/E11-LATEST-MEMORY-REPORT.json"
        recorded.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"report": str(args.report), **report["summary"], "passed": not failures}, indent=2))
    if failures:
        for failure in failures:
            print(f"BUDGET FAILED: {failure}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
