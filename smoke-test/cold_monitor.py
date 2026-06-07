#!/usr/bin/env python3
from __future__ import annotations

import datetime as dt
import json
import os
import socket
import sqlite3
import time
from pathlib import Path

OUT = Path("/tmp/scryd-pressure/raw")
OUT.mkdir(parents=True, exist_ok=True)
PREFIX = os.environ.get("SCRYD_MONITOR_PREFIX", "cold_monitor")
JSONL = OUT / f"{PREFIX}.jsonl"
SUMMARY = OUT / f"{PREFIX}_summary.json"
META = Path("/var/lib/scryd/meta.sqlite")
DATA = Path("/var/lib/scryd")
SOCK = "/run/scryd/scryd.sock"
ACCOUNT = os.environ.get("SCRYD_PRESSURE_ACCOUNT", "junk")
MAX_SECONDS = int(os.environ.get("SCRYD_MONITOR_MAX_SECONDS", "21600"))
STABLE_SECONDS = int(os.environ.get("SCRYD_MONITOR_STABLE_SECONDS", "180"))
INTERVAL = int(os.environ.get("SCRYD_MONITOR_INTERVAL", "10"))


def utc() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def uds_status() -> dict:
    req = b"GET /status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(30)
            s.connect(SOCK)
            s.sendall(req)
            chunks = []
            while True:
                b = s.recv(65536)
                if not b:
                    break
                chunks.append(b)
        raw = b"".join(chunks)
        body = raw.split(b"\r\n\r\n", 1)[1]
        return json.loads(body.decode("utf-8"))
    except Exception as e:
        return {"error": repr(e)}


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
        if p.name.isdigit():
            try:
                if (p / "comm").read_text().strip() == "scryd":
                    pid = int(p.name)
                    break
            except OSError:
                pass
    out = {"pid": pid}
    if pid:
        status = {}
        try:
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


def account(status: dict) -> dict | None:
    for a in status.get("accounts", []) if isinstance(status, dict) else []:
        if a.get("account_id") == ACCOUNT:
            return a
    return None


def main() -> int:
    start = time.monotonic()
    first_message = None
    active_at = None
    queue_zero_at = None
    last_messages = -1
    stable_since = None
    rows = 0
    with JSONL.open("a", encoding="utf-8") as f:
        while time.monotonic() - start < MAX_SECONDS:
            status = uds_status()
            acct = account(status) or {}
            db = db_stats()
            row = {
                "ts": utc(),
                "elapsed_s": round(time.monotonic() - start, 3),
                "account": acct,
                "db": db,
                "proc": proc_sample(),
            }
            f.write(json.dumps(row, sort_keys=True) + "\n")
            f.flush()
            rows += 1
            messages = int(db.get("messages") or 0)
            queue = int(db.get("queue_ready") or 0)
            if first_message is None and messages > 0:
                first_message = row["elapsed_s"]
            if active_at is None and acct.get("health") == "Active" and acct.get("last_seen_uid"):
                active_at = row["elapsed_s"]
            if queue_zero_at is None and messages > 0 and queue == 0:
                queue_zero_at = row["elapsed_s"]
            if messages != last_messages:
                last_messages = messages
                stable_since = time.monotonic()
            done = messages > 0 and queue == 0 and acct.get("health") == "Active" and stable_since and time.monotonic() - stable_since >= STABLE_SECONDS
            if done:
                break
            time.sleep(INTERVAL)
    summary = {
        "finished_at": utc(),
        "elapsed_s": round(time.monotonic() - start, 3),
        "rows": rows,
        "first_message_s": first_message,
        "active_s": active_at,
        "queue_zero_s": queue_zero_at,
        "final_status": uds_status(),
        "final_db": db_stats(),
    }
    SUMMARY.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
