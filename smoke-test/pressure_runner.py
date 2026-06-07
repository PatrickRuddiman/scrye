#!/usr/bin/env python3
"""VM-side scryd pressure runner.

Run as root on the disposable smoke VM. It preserves /var/lib/scryd/assets,
resets metadata/index/raw state when --cold-sync is requested, measures sync and
index progress, covers the public/internal UDS API, exercises the CLI, and runs
concurrent responsiveness checks. Artifacts are written to /tmp/scryd-pressure.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import glob
import json
import math
import os
import random
import shutil
import socket
import sqlite3
import statistics
import subprocess
import sys
import threading
import time
import urllib.parse
from pathlib import Path
from typing import Any

OUT = Path("/tmp/scryd-pressure")
RAW = OUT / "raw"
LOGS = OUT / "logs"
REPORT = OUT / "REPORT.md"
SOCK = Path("/run/scryd/scryd.sock")
DATA = Path("/var/lib/scryd")
META = DATA / "meta.sqlite"
WITCH = DATA / "witchcraft.sqlite"
ACCOUNT = os.environ.get("SCRYD_PRESSURE_ACCOUNT", "junk")
PASSWORD = os.environ.get("SCRYD_PRESSURE_PASSWORD", "")


def utc() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def monotonic_ms() -> int:
    return int(time.monotonic() * 1000)


def ensure_dirs() -> None:
    RAW.mkdir(parents=True, exist_ok=True)
    LOGS.mkdir(parents=True, exist_ok=True)


def run(cmd: list[str], timeout: float = 120, input_bytes: bytes | None = None) -> dict[str, Any]:
    start = time.monotonic()
    try:
        p = subprocess.run(
            cmd,
            input=input_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        return {
            "cmd": cmd,
            "rc": p.returncode,
            "ms": int((time.monotonic() - start) * 1000),
            "stdout": p.stdout.decode("utf-8", "replace"),
            "stderr": p.stderr.decode("utf-8", "replace"),
        }
    except subprocess.TimeoutExpired as e:
        return {
            "cmd": cmd,
            "rc": 124,
            "ms": int((time.monotonic() - start) * 1000),
            "stdout": (e.stdout or b"").decode("utf-8", "replace"),
            "stderr": (e.stderr or b"").decode("utf-8", "replace") + "\ntimeout",
        }


def root_cmd(args: list[str], timeout: float = 120, input_bytes: bytes | None = None) -> dict[str, Any]:
    if os.geteuid() == 0:
        return run(args, timeout, input_bytes)
    return run(["sudo", "-n", *args], timeout, input_bytes)


def read_all(sock: socket.socket) -> bytes:
    chunks: list[bytes] = []
    while True:
        try:
            chunk = sock.recv(65536)
        except socket.timeout:
            break
        if not chunk:
            break
        chunks.append(chunk)
    return b"".join(chunks)


def decode_chunked(body: bytes) -> bytes:
    out = bytearray()
    pos = 0
    while True:
        line_end = body.find(b"\r\n", pos)
        if line_end < 0:
            return bytes(out)
        size_line = body[pos:line_end].split(b";", 1)[0]
        try:
            size = int(size_line.strip(), 16)
        except ValueError:
            return body
        pos = line_end + 2
        if size == 0:
            return bytes(out)
        out.extend(body[pos:pos + size])
        pos += size + 2


def uds(method: str, path: str, timeout: float = 90, body: bytes = b"") -> dict[str, Any]:
    start = time.monotonic()
    req = (
        f"{method} {path} HTTP/1.1\r\n"
        "Host: localhost\r\n"
        "Connection: close\r\n"
        f"Content-Length: {len(body)}\r\n\r\n"
    ).encode("ascii") + body
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(timeout)
            s.connect(str(SOCK))
            s.sendall(req)
            raw = read_all(s)
    except Exception as e:  # noqa: BLE001 - artifact, not library API
        return {"ok": False, "error": repr(e), "status": 0, "ms": int((time.monotonic() - start) * 1000), "body": b""}
    split = raw.find(b"\r\n\r\n")
    if split < 0:
        return {"ok": False, "error": "malformed_http", "status": 0, "ms": int((time.monotonic() - start) * 1000), "body": raw}
    head = raw[:split].decode("iso-8859-1", "replace")
    body_bytes = raw[split + 4:]
    lines = head.split("\r\n")
    try:
        status = int(lines[0].split()[1])
    except Exception:
        status = 0
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if ":" in line:
            k, v = line.split(":", 1)
            headers[k.lower()] = v.strip().lower()
    if headers.get("transfer-encoding") == "chunked":
        body_bytes = decode_chunked(body_bytes)
    return {"ok": 200 <= status < 300, "status": status, "ms": int((time.monotonic() - start) * 1000), "body": body_bytes, "headers": headers}


def uds_json(method: str, path: str, timeout: float = 90) -> tuple[dict[str, Any], Any | None]:
    r = uds(method, path, timeout)
    try:
        return r, json.loads(r["body"].decode("utf-8"))
    except Exception:
        return r, None


def proc_pid() -> int | None:
    for p in Path("/proc").iterdir():
        if not p.name.isdigit():
            continue
        try:
            if (p / "comm").read_text().strip() == "scryd":
                return int(p.name)
        except OSError:
            pass
    return None


def proc_total_jiffies() -> int:
    parts = Path("/proc/stat").read_text().splitlines()[0].split()[1:]
    return sum(int(x) for x in parts)


def proc_jiffies(pid: int) -> int:
    parts = (Path("/proc") / str(pid) / "stat").read_text().split()
    return int(parts[13]) + int(parts[14])


def dir_size(path: Path) -> int:
    total = 0
    if not path.exists():
        return 0
    for root, _, files in os.walk(path):
        for name in files:
            try:
                total += (Path(root) / name).stat().st_size
            except OSError:
                pass
    return total


def resource_sample(prev: tuple[int, int, float] | None = None) -> tuple[dict[str, Any], tuple[int, int, float] | None]:
    pid = proc_pid()
    sample: dict[str, Any] = {"ts": utc(), "pid": pid}
    state: tuple[int, int, float] | None = None
    if pid is not None:
        status = {}
        try:
            for line in (Path("/proc") / str(pid) / "status").read_text().splitlines():
                if ":" in line:
                    k, v = line.split(":", 1)
                    status[k] = v.strip()
        except OSError:
            status = {}
        try:
            pj = proc_jiffies(pid)
            tj = proc_total_jiffies()
            now = time.monotonic()
            state = (pj, tj, now)
            cpu = None
            if prev:
                dp = pj - prev[0]
                dtj = tj - prev[1]
                if dtj > 0:
                    cpu = 100.0 * os.cpu_count() * dp / dtj
            rss_kb = int(status.get("VmRSS", "0 kB").split()[0]) if status.get("VmRSS") else 0
            sample.update({
                "cpu_pct": cpu,
                "rss_mib": rss_kb / 1024,
                "threads": int(status.get("Threads", "0")),
                "fds": len(list((Path("/proc") / str(pid) / "fd").iterdir())),
            })
        except Exception as e:  # noqa: BLE001
            sample["proc_error"] = repr(e)
    sample.update({
        "data_mib": dir_size(DATA) / 1048576,
        "raw_mib": dir_size(DATA / "raw") / 1048576,
        "assets_mib": dir_size(DATA / "assets") / 1048576,
        "meta_mib": sum(p.stat().st_size for p in DATA.glob("meta.sqlite*") if p.exists()) / 1048576,
        "witchcraft_mib": sum(p.stat().st_size for p in DATA.glob("witchcraft.sqlite*") if p.exists()) / 1048576,
    })
    return sample, state


def db_stats() -> dict[str, Any]:
    out: dict[str, Any] = {"db_exists": META.exists()}
    if not META.exists():
        return out
    try:
        con = sqlite3.connect(f"file:{META}?mode=ro", uri=True, timeout=5)
        cur = con.cursor()
        tables = [r[0] for r in cur.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
        out["tables"] = {}
        for table in tables:
            try:
                out["tables"][table] = cur.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            except sqlite3.Error as e:
                out["tables"][table] = f"error:{e}"
        if "messages" in tables:
            row = cur.execute("SELECT COUNT(*), COALESCE(SUM(size_bytes),0), MIN(date_unix), MAX(date_unix) FROM messages WHERE tombstoned_at IS NULL").fetchone()
            out.update({"messages": row[0], "message_bytes": row[1], "min_date_unix": row[2], "max_date_unix": row[3]})
            sample = cur.execute("SELECT message_id, thread_id, sender_addr, folder, subject FROM messages WHERE tombstoned_at IS NULL ORDER BY date_unix DESC LIMIT 1").fetchone()
            if sample:
                out["sample_message"] = {"message_id": sample[0], "thread_id": sample[1], "sender_addr": sample[2], "folder": sample[3], "subject": sample[4]}
        if "index_queue" in tables:
            out["index_queue_ready"] = cur.execute("SELECT COUNT(*) FROM index_queue WHERE failed_permanent=0").fetchone()[0]
            out["index_queue_failed"] = cur.execute("SELECT COUNT(*) FROM index_queue WHERE failed_permanent!=0").fetchone()[0]
        con.close()
    except Exception as e:  # noqa: BLE001
        out["error"] = repr(e)
    out["raw_files"] = sum(1 for _ in (DATA / "raw").rglob("*.eml")) if (DATA / "raw").exists() else 0
    return out


def write_json(name: str, data: Any) -> None:
    (RAW / name).write_text(json.dumps(data, indent=2, sort_keys=True), encoding="utf-8")


def wait_ready(deadline_s: float = 240) -> dict[str, Any]:
    end = time.monotonic() + deadline_s
    last: Any = None
    while time.monotonic() < end:
        r, body = uds_json("GET", "/status", timeout=30)
        last = {"http": {k: v for k, v in r.items() if k != "body"}, "body": body}
        if r.get("status") == 200 and isinstance(body, dict) and body.get("ok"):
            return last
        time.sleep(2)
    return last if isinstance(last, dict) else {"error": "not_ready"}


def cold_reset() -> dict[str, Any]:
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    backup = DATA / f"pressure-backup-{stamp}"
    backup.mkdir(parents=True, exist_ok=True)
    actions: list[dict[str, Any]] = []
    actions.append(root_cmd(["systemctl", "stop", "scryd"], timeout=420))
    for pattern in ["meta.sqlite*", "witchcraft.sqlite*", "raw"]:
        for p in DATA.glob(pattern):
            if p.name == "assets" or p.name.startswith("pressure-backup-"):
                continue
            target = backup / p.name
            try:
                shutil.move(str(p), str(target))
                actions.append({"move": str(p), "to": str(target), "ok": True})
            except Exception as e:  # noqa: BLE001
                actions.append({"move": str(p), "to": str(target), "ok": False, "error": repr(e)})
    actions.append(root_cmd(["chown", "-R", "scryd:scryd", str(DATA)], timeout=120))
    actions.append(root_cmd(["systemctl", "start", "scryd"], timeout=420))
    return {"backup": str(backup), "actions": actions, "ready": wait_ready(300)}


def measure_cold_sync(max_seconds: int) -> dict[str, Any]:
    start = time.monotonic()
    samples: list[dict[str, Any]] = []
    prev_proc = None
    first_message_at = None
    first_search_at = None
    active_at = None
    drained_at = None
    stable_since = None
    last_messages = -1
    while time.monotonic() - start < max_seconds:
        status_r, status = uds_json("GET", "/status", timeout=45)
        stats = db_stats()
        res, prev_proc = resource_sample(prev_proc)
        messages = int(stats.get("messages") or 0)
        queue = int(stats.get("index_queue_ready") or 0)
        acct = None
        if isinstance(status, dict):
            for a in status.get("accounts", []):
                if a.get("account_id") == ACCOUNT:
                    acct = a
                    break
        search = uds_json("GET", "/search?q=e&limit=1&mode=fulltext", timeout=90)[0]
        row = {
            "elapsed_s": round(time.monotonic() - start, 3),
            "status_http": status_r.get("status"),
            "health": acct.get("health") if acct else None,
            "last_seen_uid": acct.get("last_seen_uid") if acct else None,
            "last_sync_unix": acct.get("last_sync_unix") if acct else None,
            "messages": messages,
            "raw_files": stats.get("raw_files"),
            "queue": queue,
            "search_status": search.get("status"),
            "search_ms": search.get("ms"),
            "resource": res,
        }
        samples.append(row)
        if first_message_at is None and messages > 0:
            first_message_at = row["elapsed_s"]
        if first_search_at is None and search.get("status") == 200:
            first_search_at = row["elapsed_s"]
        if active_at is None and acct and acct.get("health") == "Active" and acct.get("last_seen_uid"):
            active_at = row["elapsed_s"]
        if drained_at is None and messages > 0 and queue == 0:
            drained_at = row["elapsed_s"]
        if messages != last_messages:
            last_messages = messages
            stable_since = time.monotonic()
        done = bool(messages > 0 and queue == 0 and acct and acct.get("health") == "Active" and stable_since and time.monotonic() - stable_since >= 60)
        if done:
            break
        time.sleep(5)
    result = {
        "samples": samples,
        "summary": {
            "elapsed_s": round(time.monotonic() - start, 3),
            "first_message_s": first_message_at,
            "first_search_s": first_search_at,
            "active_s": active_at,
            "index_queue_zero_s": drained_at,
            "final": samples[-1] if samples else None,
        },
    }
    write_json("cold_sync_samples.json", result)
    return result


def cli_coverage() -> list[dict[str, Any]]:
    cases: list[tuple[str, list[str], float, bytes | None]] = [
        ("version", ["scryd", "--version"], 30, None),
        ("status", ["scryd", "status"], 60, None),
        ("sync", ["scryd", "sync"], 60, None),
        ("search_fulltext", ["scryd", "search", "e", "--limit", "5", "--json"], 120, None),
        ("search_account", ["scryd", "search", "e", "--accounts", ACCOUNT, "--limit", "5", "--json"], 120, None),
        ("search_folder", ["scryd", "search", "e", "--folder", "INBOX", "--limit", "5", "--json"], 120, None),
        ("search_dates", ["scryd", "search", "e", "--since", "2020-01-01", "--until", "2030-01-01", "--limit", "5", "--json"], 120, None),
        ("search_semantic", ["scryd", "search", "invoice", "--mode", "semantic", "--limit", "5", "--json"], 180, None),
        ("search_hybrid", ["scryd", "search", "invoice", "--mode", "hybrid", "--limit", "5", "--json"], 180, None),
        ("bad_mode", ["scryd", "search", "x", "--mode", "bogus"], 30, None),
    ]
    if PASSWORD:
        cases.append(("rotate_same_password", ["scryd", "rotate-password", ACCOUNT, "--password-stdin"], 60, PASSWORD.encode()))
    out = []
    for name, cmd, timeout, stdin in cases:
        r = root_cmd(cmd, timeout, stdin)
        r["name"] = name
        out.append(r)
    write_json("cli_coverage.json", out)
    return out


def endpoint_coverage() -> dict[str, Any]:
    results: dict[str, Any] = {"cases": []}

    def add(name: str, method: str, path: str, timeout: float = 120) -> tuple[dict[str, Any], Any | None]:
        r, body = uds_json(method, path, timeout)
        item = {"name": name, "method": method, "path": path, "status": r.get("status"), "ms": r.get("ms"), "json": isinstance(body, (dict, list))}
        if not item["json"]:
            item["body_prefix"] = r.get("body", b"")[:200].decode("utf-8", "replace")
        results["cases"].append(item)
        return r, body

    add("status", "GET", "/status")
    add("accounts", "GET", "/accounts")
    _, full = add("search_fulltext", "GET", "/search?q=e&limit=5&mode=fulltext", 180)
    add("search_account", "GET", f"/search?q=e&account_ids={urllib.parse.quote(ACCOUNT)}&limit=5&mode=fulltext", 180)
    add("search_folder", "GET", "/search?q=e&folder=INBOX&limit=5&mode=fulltext", 180)
    add("search_dates", "GET", "/search?q=e&since=2020-01-01&until=2030-01-01&limit=5&mode=fulltext", 180)
    add("search_semantic", "GET", "/search?q=invoice&limit=5&mode=semantic", 240)
    add("search_hybrid", "GET", "/search?q=invoice&limit=5&mode=hybrid", 240)
    add("bad_mode", "GET", "/search?q=e&mode=bogus")
    add("bad_since", "GET", "/search?q=e&since=not-a-date")
    hit = None
    if isinstance(full, dict) and full.get("hits"):
        hit = full["hits"][0]
        mid = urllib.parse.quote(hit["message_id"], safe="")
        tid = urllib.parse.quote(hit["thread_id"], safe="")
        r = uds("GET", f"/message/{mid}", 180)
        results["cases"].append({"name": "message", "method": "GET", "path": "/message/<hit>", "status": r.get("status"), "ms": r.get("ms"), "bytes": len(r.get("body", b""))})
        r = uds("GET", f"/message/{mid}/raw", 180)
        results["cases"].append({"name": "message_raw", "method": "GET", "path": "/message/<hit>/raw", "status": r.get("status"), "ms": r.get("ms"), "bytes": len(r.get("body", b""))})
        add("thread", "GET", f"/thread/{tid}", 180)
    add("missing_message", "GET", "/message/does-not-exist")
    add("missing_thread", "GET", "/thread/does-not-exist")
    add("sync", "POST", "/sync")
    add("reconcile", "POST", "/internal/reconcile")
    results["selected_hit"] = hit
    write_json("endpoint_coverage.json", results)
    return results


def pct(values: list[float], p: float) -> float | None:
    if not values:
        return None
    vals = sorted(values)
    idx = min(len(vals) - 1, max(0, math.ceil((p / 100) * len(vals)) - 1))
    return vals[idx]


def load_test(name: str, paths: list[str], concurrency: int, duration_s: int, timeout_s: float) -> dict[str, Any]:
    stop = time.monotonic() + duration_s
    lock = threading.Lock()
    rows: list[dict[str, Any]] = []
    samples: list[dict[str, Any]] = []
    prev_proc = None

    def worker(worker_id: int) -> None:
        rnd = random.Random(worker_id + int(time.time()))
        while time.monotonic() < stop:
            path = rnd.choice(paths)
            r = uds("GET", path, timeout_s)
            row = {"path": path, "status": r.get("status"), "ms": r.get("ms"), "error": r.get("error")}
            with lock:
                rows.append(row)

    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
        futs = [ex.submit(worker, i) for i in range(concurrency)]
        while time.monotonic() < stop:
            s, prev_proc = resource_sample(prev_proc)
            samples.append(s)
            time.sleep(2)
        for f in futs:
            f.result(timeout=timeout_s + 5)

    lats = [float(r["ms"]) for r in rows if r.get("status") and 200 <= int(r["status"]) < 300]
    errors = [r for r in rows if not (r.get("status") and 200 <= int(r["status"]) < 300)]
    result = {
        "name": name,
        "concurrency": concurrency,
        "duration_s": duration_s,
        "requests": len(rows),
        "successes": len(lats),
        "errors": len(errors),
        "rps": len(rows) / max(duration_s, 1),
        "latency_ms": {"min": min(lats) if lats else None, "p50": pct(lats, 50), "p95": pct(lats, 95), "p99": pct(lats, 99), "max": max(lats) if lats else None},
        "status_counts": {str(code): sum(1 for r in rows if r.get("status") == code) for code in sorted({r.get("status") for r in rows})},
        "sample_errors": errors[:10],
        "resource_samples": samples,
    }
    write_json(f"load_{name}.json", result)
    return result


def reindex_measure(max_seconds: int) -> dict[str, Any]:
    start = time.monotonic()
    trigger = root_cmd(["scryd", "reindex"], timeout=300)
    samples: list[dict[str, Any]] = []
    prev_proc = None
    queue_zero_at = None
    search_ok_at = None
    while time.monotonic() - start < max_seconds:
        stats = db_stats()
        res, prev_proc = resource_sample(prev_proc)
        search = uds_json("GET", "/search?q=e&limit=1&mode=fulltext", timeout=180)[0]
        queue = int(stats.get("index_queue_ready") or 0)
        row = {"elapsed_s": round(time.monotonic() - start, 3), "queue": queue, "messages": stats.get("messages"), "search_status": search.get("status"), "search_ms": search.get("ms"), "resource": res}
        samples.append(row)
        if search_ok_at is None and search.get("status") == 200:
            search_ok_at = row["elapsed_s"]
        if queue_zero_at is None and queue == 0 and (stats.get("messages") or 0) > 0:
            queue_zero_at = row["elapsed_s"]
            break
        time.sleep(5)
    second = uds_json("POST", "/internal/reindex", timeout=120)[0]
    result = {"trigger": trigger, "samples": samples, "summary": {"elapsed_s": round(time.monotonic() - start, 3), "queue_zero_s": queue_zero_at, "search_ok_s": search_ok_at, "second_reindex_status": second.get("status")}}
    write_json("reindex_measure.json", result)
    return result


def summarize_resources(samples: list[dict[str, Any]]) -> dict[str, Any]:
    flat = []
    for s in samples:
        if "resource" in s and isinstance(s["resource"], dict):
            flat.append(s["resource"])
        else:
            flat.append(s)
    rss = [float(s.get("rss_mib")) for s in flat if s.get("rss_mib") is not None]
    cpu = [float(s.get("cpu_pct")) for s in flat if s.get("cpu_pct") is not None]
    return {"rss_mib_max": max(rss) if rss else None, "rss_mib_p95": pct(rss, 95), "cpu_pct_max": max(cpu) if cpu else None, "cpu_pct_p95": pct(cpu, 95)}


def report(all_results: dict[str, Any]) -> None:
    lines: list[str] = []
    lines.append("# scryd pressure test report")
    lines.append("")
    lines.append(f"Generated: {utc()}")
    lines.append("")
    lines.append("## Environment")
    lines.append("")
    lines.append(f"- Account: `{ACCOUNT}`")
    lines.append(f"- Python: `{sys.version.split()[0]}`")
    lines.append(f"- CPUs: `{os.cpu_count()}`")
    lines.append(f"- Kernel: `{os.uname().sysname} {os.uname().release}`")
    for name, cmd in [("scryd", ["scryd", "--version"]), ("service", ["systemctl", "is-active", "scryd"]), ("status", ["systemctl", "show", "scryd", "-p", "ActiveEnterTimestamp", "-p", "TimeoutStopUSec"] )]:
        r = root_cmd(cmd, timeout=30)
        lines.append(f"- {name}: `{(r['stdout'] or r['stderr']).strip()}`")
    lines.append("")
    final_stats = db_stats()
    lines.append("## Corpus")
    lines.append("")
    lines.append(f"- Messages: `{final_stats.get('messages')}`")
    lines.append(f"- Raw `.eml` files: `{final_stats.get('raw_files')}`")
    lines.append(f"- Pending index queue: `{final_stats.get('index_queue_ready')}`")
    lines.append(f"- Failed index queue: `{final_stats.get('index_queue_failed')}`")
    lines.append(f"- Stored message bytes: `{final_stats.get('message_bytes')}`")
    lines.append(f"- Data size MiB: `{resource_sample()[0].get('data_mib'):.1f}`")
    lines.append("")
    cold = all_results.get("cold_sync")
    if cold:
        s = cold.get("summary", {})
        final = s.get("final") or {}
        lines.append("## Cold sync + indexing")
        lines.append("")
        lines.append(f"- Elapsed seconds: `{s.get('elapsed_s')}`")
        lines.append(f"- First message seconds: `{s.get('first_message_s')}`")
        lines.append(f"- First searchable seconds: `{s.get('first_search_s')}`")
        lines.append(f"- Active health seconds: `{s.get('active_s')}`")
        lines.append(f"- Index queue zero seconds: `{s.get('index_queue_zero_s')}`")
        lines.append(f"- Final messages: `{final.get('messages')}`")
        lines.append(f"- Final last_seen_uid: `{final.get('last_seen_uid')}`")
        lines.append(f"- Final health: `{final.get('health')}`")
        lines.append("")
    lines.append("## Endpoint coverage")
    lines.append("")
    lines.append("| case | status | ms | json |")
    lines.append("| --- | ---: | ---: | --- |")
    for c in (all_results.get("endpoints") or {}).get("cases", []):
        lines.append(f"| {c['name']} | {c.get('status')} | {c.get('ms')} | {c.get('json', '')} |")
    lines.append("")
    lines.append("## CLI coverage")
    lines.append("")
    lines.append("| case | rc | ms |")
    lines.append("| --- | ---: | ---: |")
    for c in all_results.get("cli", []):
        lines.append(f"| {c['name']} | {c.get('rc')} | {c.get('ms')} |")
    lines.append("")
    lines.append("## Load results")
    lines.append("")
    lines.append("| phase | concurrency | requests | errors | rps | p50 ms | p95 ms | p99 ms | max ms |")
    lines.append("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
    for l in all_results.get("loads", []):
        lat = l.get("latency_ms", {})
        lines.append(f"| {l['name']} | {l['concurrency']} | {l['requests']} | {l['errors']} | {l['rps']:.2f} | {lat.get('p50')} | {lat.get('p95')} | {lat.get('p99')} | {lat.get('max')} |")
    lines.append("")
    reidx = all_results.get("reindex")
    if reidx:
        lines.append("## Reindex")
        lines.append("")
        lines.append(f"- Trigger rc: `{reidx.get('trigger', {}).get('rc')}`")
        lines.append(f"- Queue zero seconds: `{reidx.get('summary', {}).get('queue_zero_s')}`")
        lines.append(f"- Search available seconds: `{reidx.get('summary', {}).get('search_ok_s')}`")
        lines.append(f"- Second reindex HTTP status: `{reidx.get('summary', {}).get('second_reindex_status')}`")
        lines.append("")
    resource_groups = []
    if cold:
        resource_groups.extend(cold.get("samples", []))
    if reidx:
        resource_groups.extend(reidx.get("samples", []))
    for l in all_results.get("loads", []):
        resource_groups.extend(l.get("resource_samples", []))
    lines.append("## Resource envelope")
    lines.append("")
    lines.append("```json")
    lines.append(json.dumps(summarize_resources(resource_groups), indent=2, sort_keys=True))
    lines.append("```")
    lines.append("")
    lines.append("## Raw artifacts")
    lines.append("")
    for p in sorted(RAW.glob("*.json")):
        lines.append(f"- `{p}`")
    REPORT.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cold-sync", action="store_true")
    ap.add_argument("--max-cold-seconds", type=int, default=7200)
    ap.add_argument("--read-seconds", type=int, default=60)
    ap.add_argument("--mixed-seconds", type=int, default=120)
    ap.add_argument("--semantic-seconds", type=int, default=60)
    ap.add_argument("--reindex-max-seconds", type=int, default=3600)
    ap.add_argument("--skip-reindex", action="store_true")
    args = ap.parse_args()

    ensure_dirs()
    results: dict[str, Any] = {"started": utc()}
    write_json("initial_db_stats.json", db_stats())

    if args.cold_sync:
        results["cold_reset"] = cold_reset()
        results["cold_sync"] = measure_cold_sync(args.max_cold_seconds)
    else:
        results["ready"] = wait_ready(300)

    results["db_after_ready"] = db_stats()
    results["cli"] = cli_coverage()
    # rotate-password writes config; restart to make sure the service remains healthy.
    if PASSWORD:
        results["post_rotate_restart"] = root_cmd(["systemctl", "restart", "scryd"], timeout=420)
        results["post_rotate_ready"] = wait_ready(300)
    results["endpoints"] = endpoint_coverage()

    paths_read = ["/status", "/accounts", "/search?q=e&limit=3&mode=fulltext"]
    paths_mixed = [
        "/status", "/accounts", "/search?q=e&limit=3&mode=fulltext",
        f"/search?q=e&account_ids={urllib.parse.quote(ACCOUNT)}&limit=3&mode=fulltext",
        "/search?q=invoice&limit=3&mode=hybrid", "/search?q=invoice&limit=3&mode=semantic",
    ]
    hit = (results.get("endpoints") or {}).get("selected_hit")
    if hit:
        mid = urllib.parse.quote(hit["message_id"], safe="")
        tid = urllib.parse.quote(hit["thread_id"], safe="")
        paths_mixed.extend([f"/message/{mid}", f"/message/{mid}/raw", f"/thread/{tid}"])

    results["loads"] = [
        load_test("read_status_search", paths_read, concurrency=24, duration_s=args.read_seconds, timeout_s=120),
        load_test("mixed_public_api", paths_mixed, concurrency=16, duration_s=args.mixed_seconds, timeout_s=180),
        load_test("semantic_hybrid", ["/search?q=invoice&limit=5&mode=semantic", "/search?q=invoice&limit=5&mode=hybrid"], concurrency=4, duration_s=args.semantic_seconds, timeout_s=240),
    ]

    if args.skip_reindex:
        results["reindex"] = None
        results["post_reindex_restart"] = None
        results["post_reindex_ready"] = None
    else:
        results["reindex"] = reindex_measure(args.reindex_max_seconds)
        # Restart once after reindex because the current implementation retains the single-flight marker in memory.
        results["post_reindex_restart"] = root_cmd(["systemctl", "restart", "scryd"], timeout=420)
        results["post_reindex_ready"] = wait_ready(300)
    results["final_db_stats"] = db_stats()
    results["finished"] = utc()
    write_json("all_results.json", results)
    report(results)
    print(str(REPORT))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
