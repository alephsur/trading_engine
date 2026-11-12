#!/usr/bin/env python3
"""Independently reconcile an OHLC approximation report against its fills and funding."""
import json
import sys
from decimal import Decimal as D
from pathlib import Path

path = Path(sys.argv[1])
report = json.loads(path.read_text())
data_root = Path(sys.argv[2]) if len(sys.argv) > 2 else path.parent
for run in report['results']:
    cash = D(run['capital'])
    positions = {}
    fees = D(0)
    for order in run['orders']:
        key = order['position_id']
        for fill in order['fills']:
            qty, price, fee = (D(fill[k]) for k in ('quantity', 'price', 'fee'))
            fees += fee
            cash -= fee
            if order['reduce_only']:
                p = positions[key]
                assert qty <= p['qty']
                cash += (price - p['entry']) * qty * p['sign']
                p['qty'] -= qty
            else:
                assert key not in positions, 'unexpected partial/multiple entry'
                positions[key] = dict(qty=qty, entry=price, sign=D(1) if order['buy'] else D(-1))
    funding = sum((D(f['cash_change']) for f in run['funding']), D(0))
    cash += funding
    half = D(run['spread_bps']) / D(20000)
    equity = cash
    for p in run['open_positions']:
        candles = json.loads((data_root / f"{p['symbol']}-{run['interval']}.json").read_text())
        price = D(candles[-1]['c']) * (1 - half if p['long'] else 1 + half)
        equity += (price - D(p['entry'])) * D(p['quantity']) * (1 if p['long'] else -1)
        assert positions[p['id']]['qty'] == D(p['quantity'])
    assert abs(equity - D(run['final_equity'])) < D('0.00000001')
    assert fees == D(run['fees'])
    assert funding == D(run['funding_cash_flow'])
    assert len([p for p in positions.values() if p['qty'] == 0]) == run['closed_trades']
    assert len([p for p in positions.values() if p['qty'] > 0]) == len(run['open_positions'])
    print('PASS', run['interval'], run['spread_bps'], 'bps: equity, fees, funding, quantities')
