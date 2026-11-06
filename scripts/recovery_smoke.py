#!/usr/bin/env python3
"""Destructive fixture test: run only against the disposable Compose CI database."""
import json
import subprocess
import urllib.request


def sql(query):
    return subprocess.check_output(
        ["docker", "compose", "exec", "-T", "postgres", "psql", "-U", "trading", "-d", "trading", "-Atc", query],
        text=True,
    ).strip()


def get(path):
    with urllib.request.urlopen("http://127.0.0.1:3000" + path, timeout=5) as response:
        return json.load(response)


# Stop ingestion before deleting one historical candle to exercise recovery on restart.
subprocess.run(["docker", "compose", "stop", "engine"], check=True)
original = json.loads(sql("SELECT payload FROM candles WHERE symbol='BTC' AND interval='1m' AND finalized ORDER BY open_time LIMIT 1"))
anchor = sql("SELECT start_time FROM candle_coverage WHERE symbol='BTC' AND interval='1m'")
opened = int(original["t"])
sql(f"DELETE FROM candles WHERE symbol='BTC' AND interval='1m' AND open_time={opened}")
subprocess.run(["docker", "compose", "start", "engine"], check=True)
subprocess.run(["python3", "scripts/smoke.py"], check=True)
restored = json.loads(sql(f"SELECT payload FROM candles WHERE symbol='BTC' AND interval='1m' AND open_time={opened}"))
assert restored == original
assert sql("SELECT start_time FROM candle_coverage WHERE symbol='BTC' AND interval='1m'") == anchor
assert sql("SELECT count(*) FROM candle_coverage WHERE missing_count > 0") == "0"
assert get("/health")["history_ready"]
print("PASS: restart repairs an internal candle gap and preserves the coverage anchor")
