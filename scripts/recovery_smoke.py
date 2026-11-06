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
saved_paper = json.loads(sql("SELECT state FROM paper_accounts WHERE account_id='paper-default'"))
saved_orders = json.loads(sql("SELECT coalesce(jsonb_agg(payload ORDER BY order_id),'[]') FROM paper_orders WHERE account_id='paper-default'"))
saved_evaluation = json.loads(sql("SELECT payload FROM evaluations WHERE symbol='BTC' AND interval='1m' ORDER BY open_time DESC LIMIT 1"))
saved_signal_counts = sql("SELECT count(*) FROM signals")
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

config_id = saved_evaluation["config_id"]
assert len(config_id) == 64 and all(c in "0123456789abcdef" for c in config_id)
evaluated_open = int(saved_evaluation["open_time"])
same = json.loads(sql(f"SELECT payload FROM evaluations WHERE config_id='{config_id}' AND symbol='BTC' AND interval='1m' AND open_time={evaluated_open}"))
assert same == saved_evaluation
assert sql("SELECT count(*) FROM (SELECT config_id,symbol,interval,open_time,strategy,count(*) FROM signals GROUP BY 1,2,3,4,5 HAVING count(*) > 1) duplicates") == "0"
assert int(sql("SELECT count(*) FROM signals")) >= int(saved_signal_counts)
print("PASS: restart preserves evaluations and does not duplicate directional events")

recovered_paper = get('/paper/account')['account']
assert recovered_paper['activated_at'] == saved_paper['activated_at']
assert recovered_paper['config'] == saved_paper['config']
assert recovered_paper['sequence'] >= saved_paper['sequence']
orders = json.loads(sql("SELECT coalesce(jsonb_agg(payload ORDER BY order_id),'[]') FROM paper_orders WHERE account_id='paper-default'"))
assert orders[:len(saved_orders)] == saved_orders
assert len({o['id'] for o in orders}) == len(orders)
print('PASS: restart preserves paper account identity and immutable order/fill history')
