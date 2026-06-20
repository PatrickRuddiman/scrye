#!/usr/bin/env python3
"""Minimal MCP Streamable-HTTP client for the scryd e2e smoke test.

scryd's only search surface is the MCP server on loopback TCP, so the e2e drives
it the way a real MCP client would: initialize, list tools, call `search`. This
implements just enough of the Streamable-HTTP transport (JSON-RPC over POST,
SSE-or-JSON response parsing, session-id + protocol-version headers) using only
the Python standard library — no third-party deps on the CI runner.

Usage:
    mcp_client.py <base_url> tools
    mcp_client.py <base_url> search <query> [--limit N] [--accounts a,b] [--mode m]

On success prints the tool result as JSON to stdout and exits 0. On any
transport/JSON-RPC error prints a diagnostic to stderr and exits non-zero.
"""

import argparse
import json
import sys
import urllib.error
import urllib.request

PROTOCOL_VERSION = "2025-06-18"


class McpError(RuntimeError):
    pass


class McpHttpClient:
    def __init__(self, base_url):
        self.base_url = base_url
        self.session_id = None
        self.protocol_version = PROTOCOL_VERSION
        self._next_id = 0

    def _headers(self):
        h = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            "MCP-Protocol-Version": self.protocol_version,
        }
        if self.session_id:
            h["Mcp-Session-Id"] = self.session_id
        return h

    def _post(self, payload, is_request):
        body = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            self.base_url, data=body, headers=self._headers(), method="POST"
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                sid = resp.headers.get("Mcp-Session-Id")
                if sid:
                    self.session_id = sid
                ctype = (resp.headers.get("Content-Type") or "").lower()
                raw = resp.read().decode("utf-8", "replace")
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace") if e.fp else ""
            raise McpError(f"HTTP {e.code} from MCP server: {detail or e.reason}")
        except urllib.error.URLError as e:
            raise McpError(f"cannot reach MCP server at {self.base_url}: {e.reason}")

        if not is_request:
            return None
        msg = _extract_jsonrpc(raw, ctype)
        if msg is None:
            raise McpError(f"no JSON-RPC response in body (content-type {ctype!r}): {raw!r}")
        if "error" in msg:
            raise McpError(f"JSON-RPC error: {json.dumps(msg['error'])}")
        return msg.get("result")

    def _request(self, method, params):
        self._next_id += 1
        return self._post(
            {"jsonrpc": "2.0", "id": self._next_id, "method": method, "params": params},
            is_request=True,
        )

    def _notify(self, method, params=None):
        payload = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            payload["params"] = params
        self._post(payload, is_request=False)

    def initialize(self):
        result = self._request(
            "initialize",
            {
                "protocolVersion": self.protocol_version,
                "capabilities": {},
                "clientInfo": {"name": "scryd-e2e", "version": "0"},
            },
        )
        negotiated = (result or {}).get("protocolVersion")
        if negotiated:
            self.protocol_version = negotiated
        self._notify("notifications/initialized")
        return result

    def list_tools(self):
        result = self._request("tools/list", {}) or {}
        return [t.get("name") for t in result.get("tools", [])]

    def call_tool(self, name, arguments):
        result = self._request("tools/call", {"name": name, "arguments": arguments}) or {}
        if result.get("isError"):
            raise McpError(f"tool {name} returned isError: {json.dumps(result)}")
        for block in result.get("content", []):
            if block.get("type") == "text":
                return json.loads(block["text"])
        raise McpError(f"tool {name} returned no text content: {json.dumps(result)}")


def _extract_jsonrpc(raw, ctype):
    """Return the JSON-RPC object from a JSON body or an SSE event stream."""
    raw = raw.strip()
    if not raw:
        return None
    if "text/event-stream" in ctype or raw.startswith("event:") or raw.startswith("data:"):
        for line in raw.splitlines():
            line = line.strip()
            if line.startswith("data:"):
                data = line[len("data:"):].strip()
                if not data:
                    continue
                try:
                    obj = json.loads(data)
                except json.JSONDecodeError:
                    continue
                if isinstance(obj, dict) and ("result" in obj or "error" in obj):
                    return obj
        return None
    return json.loads(raw)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("base_url")
    sub = ap.add_subparsers(dest="command", required=True)
    sub.add_parser("tools")
    sp = sub.add_parser("search")
    sp.add_argument("query")
    sp.add_argument("--limit", type=int, default=20)
    sp.add_argument("--accounts", default="")
    sp.add_argument("--mode", default=None)
    args = ap.parse_args(argv)

    client = McpHttpClient(args.base_url)
    try:
        client.initialize()
        if args.command == "tools":
            print(json.dumps(client.list_tools()))
            return 0
        if args.command == "search":
            arguments = {"q": args.query, "limit": args.limit}
            if args.accounts:
                arguments["account_ids"] = [a for a in args.accounts.split(",") if a]
            if args.mode:
                arguments["mode"] = args.mode
            print(json.dumps(client.call_tool("search", arguments)))
            return 0
    except McpError as e:
        print(f"mcp_client: {e}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
