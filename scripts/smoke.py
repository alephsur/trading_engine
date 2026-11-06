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


for attempt in range(180):
    try:
        health = get("/health")
        if health["status"] == "ready":
            break
    except (urllib.error.URLError, TimeoutError):
        pass
    time.sleep(1)
else:
    raise SystemExit("Engine did not become ready within 180 attempts")

assert health["mode"] == "PAPER" and health["execution_enabled"] is False
assert health["analysis_ready"]
strategies = get("/strategies")
assert len(strategies["manifest"]["strategies"]) == 4
assert len(strategies["cursors"]) == 16
assert strategies["analysis_only"]
assert get("/livez")["alive"] is True
markets = get("/markets")
assert {m["metadata"]["name"] for m in markets} == {"BTC", "ETH", "SOL", "HYPE"}
assert len({m["asset_index"] for m in markets}) == 4
prices = get("/prices")
assert prices["fresh"]
assert set(prices["prices"]) == {"BTC", "ETH", "SOL", "HYPE"}
assert all(isinstance(p, str) and Decimal(p) > 0 for p in prices["prices"].values())
for symbol in ("BTC", "ETH", "SOL", "HYPE"):
    book = get("/book?symbol=" + symbol)
    assert book["fresh"] and book["book"]["coin"] == symbol
    assert all(book["book"]["levels"])
    trades = get("/trades?symbol=" + symbol)["trades"]
    assert trades and all(t["coin"] == symbol for t in trades)
    for interval in ("1m", "5m", "15m", "1h"):
        candles = get(f"/candles?symbol={symbol}&interval={interval}")["candles"]
        assert len(candles) == 100
        assert all(c["closed"] and c["candle"]["s"] == symbol for c in candles)
        times = [c["candle"]["t"] for c in candles]
        assert times == sorted(set(times), reverse=True)
        evaluation = get(f"/evaluations?symbol={symbol}&interval={interval}&limit=1")["evaluations"][0]
        assert evaluation["analysis_only"] and evaluation["config_id"] == strategies["config_id"]
        assert len(evaluation["decisions"]) == 4 and evaluation["indicators"] is not None
        assert evaluation["close_time"] < int(time.time() * 1000)
        signals = get(f"/signals?symbol={symbol}&interval={interval}")["signals"]
        assert all(s["analysis_only"] and s["direction"] in ("long", "short") for s in signals)
        assert len({s["event_id"] for s in signals}) == len(signals)
status = get("/data-status")
assert status["history_ready"]
assert len(status["candle_coverage"]) == 16
assert all(c["missing_count"] == 0 for c in status["candle_coverage"])
for query in ("symbol=DOGE", "symbol=BTC&interval=2m", "symbol=BTC&limit=0"):
    try:
        get("/candles?" + query)
        raise AssertionError("invalid query accepted")
    except urllib.error.HTTPError as error:
        assert error.code == 400
print("PASS: real prices, books, trades, 16 closed candle series, indicators, strategy evaluations and analysis-only signals")
paper = get('/paper/account')
assert paper['mode'] == 'PAPER' and not paper['real_execution_enabled']
account = paper['account']
assert account['config']['capital'] == '10000'
assert account['config']['risk_fraction'] == '0.0025'
assert len(account['positions']) <= 2
assert account['last_tick'] >= int(time.time() * 1000) - 15000
assert get('/health')['paper_ready']
assert all(p['stop'] and p['take_profit'] for p in get('/paper/positions')['positions'])
for kind in ('orders', 'fills', 'decisions', 'funding', 'events'):
    assert isinstance(get(f'/paper/{kind}?limit=10')[kind], list)
print('PASS: persistent PAPER account, risk limits, worker freshness and audit endpoints')
