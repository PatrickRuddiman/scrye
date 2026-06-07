#!/usr/bin/env python3
from __future__ import annotations

import datetime as dt
import json
import os
import socket
import sqlite3
import subprocess
import time
from pathlib import Path

OUT = Path("/tmp/scryd-pressure/raw")
OUT.mkdir(parents=True, exist_ok=True)
JSONL = OUT / "reindex_monitor.jsonl"
SUMMARY = OUT / "reindex_monitor_summary.json"
META = Path("/var/lib/scryd/meta.sqlite")
DATA = Path("/var/lib/scryd")
SOCK = "/run/scryd/scryd.sock"
MAX_SECONDS = int(os.environ.get("SCRYD_REINDEX_MAX_SECONDS", "21600"))
STABLE_SECONDS = int(os.environ.get("SCRYD_REINDEX_STABLE_SECONDS", "120"))
INTERVAL = int(os.environ.get("SCRYD_REINDEX_INTERVAL", "10"))
SEARCH_INTERVAL = int(os.environ.get("SCRYD_REINDEX_SEARCH_INTERVAL", "60"))


def utc() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def run(cmd: list[str], timeout: int = 300) -> dict:
    start = time.monotonic()
    try:
        p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout, check=False)
        return {"cmd": cmd, "rc": p.returncode, "ms": int((time.monotonic() - start) * 1000), "stdout": p.stdout.decode("utf-8", "replace"), "stderr": p.stderr.decode("utf-8", "replace")}
    except subprocess.TimeoutExpired as e:
        return {"cmd": cmd, "rc": 124, "ms": int((time.monotonic() - start) * 1000), "stdout": (e.stdout or b"").decode("utf-8", "replace"), "stderr": (e.stderr or b"").decode("utf-8", "replace") + "\ntimeout"}


def request(method: str, path: str, timeout: int = 180) -> dict:
    start = time.monotonic()
    req = f"{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".encode("ascii")
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(timeout)
            s.connect(SOCK)
            s.sendall(req)
            chunks = []
            while True:
                b = s.recv(65536)
                if not b:
                    break
                chunks.append(b)
        raw = b"".join(chunks)
        head, _, body = raw.partition(b"\r\n\r\n")
        status = int(head.split(maxsplit=2)[1]) if head else 0
        return {"status": status, "ms": int((time.monotonic() - start) * 1000), "bytes": len(body)}
    except Exception as e:
        return {"status": 0, "ms": int((time.monotonic() - start) * 1000), "error": repr(e), "bytes": 0}


def db_stats() -> dict:
    out = {"db_exists": META.exists()}
    if not META.exists():
        return out
    try:
        con = sqlite3.connect(f"file:{META}?mode=ro", uri=True, timeout=5)
        c = con.cursor()
        for t in ["accounts", "sync_state", "messages", "index_queue"]:
            out[t] = c.execute(f"select count(*) from {t}").fetchone()[0]
        out["queue_ready"] = c.execute("select count(*) from index_queue where failed_permanent=0").fetchone()[0]
        out["queue_failed"] = c.execute("select count(*) from index_queue where failed_permanent!=0").fetchone()[0]
        out["message_bytes"] = c.execute("select coalesce(sum(size_bytes),0) from messages where tombstoned_at is null").fetchone()[0]
        con.close()
    except Exception as e:
        out["db_error"] = repr(e)
    raw = DATA / "raw"
    out["raw_files"] = sum(1 for _ in raw.rglob("*.eml")) if raw.exists() else 0
    return out


def proc_sample() -> dict:
    pid = None
    for p in Path("/proc").iterdir():
        if not p.name.isdigit():
            continue
        try:
            if (p / "comm").read_text().strip() == "scryd":
                pid = int(p.name)
                break
        except OSError:
            pass
    out = {"pid": pid}
    if pid:
        try:
            status = {}
            for line in (Path("/proc") / str(pid) / "status").read_text().splitlines():
                if ":" in line:
                    k, v = line.split(":", 1)
                    status[k] = v.strip()
            out["rss_mib"] = int(status.get("VmRSS", "0 kB").split()[0]) / 1024
            out["threads"] = int(status.get("Threads", "0"))
            out["fds"] = len(list((Path("/proc") / str(pid) / "fd").iterdir()))
        except Exception as e:
            out["proc_error"] = repr(e)
    return out


def main() -> int:
    started = utc()
    before = db_stats()
    trigger = run(["scryd", "reindex"], timeout=300)
    second = request("POST", "/internal/reindex", timeout=60)
    start = time.monotonic()
    queue_zero_at = None
    search_ok_at = None
    stable_since = None
    last_queue = None
    last_search = None
    rows = 0
    with JSONL.open("a", encoding="utf-8") as f:
        while time.monotonic() - start < MAX_SECONDS:
            elapsed = time.monotonic() - start
            db = db_stats()
            queue = int(db.get("queue_ready") or 0)
            if last_search is None or elapsed - last_search >= SEARCH_INTERVAL:
                search = request("GET", "/search?q=e&limit=1&mode=fulltext", timeout=180)
                last_search = elapsed
                if search_ok_at is None and search.get("status") == 200:
                    search_ok_at = round(elapsed, 3)
            else:
                search = None
            row = {"ts": utc(), "elapsed_s": round(elapsed, 3), "db": db, "proc": proc_sample(), "search": search}
            f.write(json.dumps(row, sort_keys=True) + "\n")
            f.flush()
            rows += 1
            if queue != last_queue:
                last_queue = queue
                stable_since = time.monotonic()
            if queue_zero_at is None and queue == 0 and (db.get("messages") or 0) > 0:
                queue_zero_at = round(elapsed, 3)
            if queue == 0 and stable_since and time.monotonic() - stable_since >= STABLE_SECONDS:
                break
            time.sleep(INTERVAL)
    after_zero_second = request("POST", "/internal/reindex", timeout=60)
    restart = run(["systemctl", "restart", "scryd"], timeout=420)
    ready = request("GET", "/status", timeout=60)
    summary = {
        "started_at": started,
        "finished_at": utc(),
        "elapsed_s": round(time.monotonic() - start, 3),
        "rows": rows,
        "before": before,
        "trigger": trigger,
        "second_reindex_immediate": second,
        "queue_zero_s": queue_zero_at,
        "search_ok_s": search_ok_at,
        "after_zero_second_reindex": after_zero_second,
        "restart": restart,
        "ready_after_restart": ready,
        "final_db": db_stats(),
    }
    SUMMARY.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
