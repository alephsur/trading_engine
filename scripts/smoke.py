#!/usr/bin/env python3
"""Check a running engine against real public data; no account required."""
import json
import sys
import time
import urllib.error
import urllib.request
from decimal import Decimal

base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:3000"


def get(path):
    with urllib.request.urlopen(base + path, timeout=4) as response:
        return json.load(response)


for attempt in range(30):
    try:
        health = get("/health")
        if health["status"] == "ready":
            break
    except (urllib.error.URLError, TimeoutError):
        pass
    time.sleep(1)
else:
    raise SystemExit("Engine did not become ready within 30 attempts")

assert health["mode"] == "PAPER" and health["execution_enabled"] is False
assert get("/livez")["alive"] is True
markets = get("/markets")
assert {m["metadata"]["name"] for m in markets} == {"BTC", "ETH", "SOL", "HYPE"}
assert len({m["asset_index"] for m in markets}) == 4
prices = get("/prices")
assert prices["fresh"]
assert set(prices["prices"]) == {"BTC", "ETH", "SOL", "HYPE"}
assert all(isinstance(p, str) and Decimal(p) > 0 for p in prices["prices"].values())
print("PASS: health, four dynamic markets, fresh decimal prices, execution disabled")
