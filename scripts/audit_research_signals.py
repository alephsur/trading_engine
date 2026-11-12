#!/usr/bin/env python3
"""Independent check of benchmark entries using only bars closed before the order."""
import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'artifacts/five-agent-research-2026-11-11'
folders=sorted((ROOT/'artifacts/prior-30d-2026-11-11').glob('20*'))+[ROOT/'artifacts/backtest-30d-2026-11-11']
def ema(xs,n):
    vs=[None]*(n-1);x=sum(xs[:n])/n;vs.append(x)
    for p in xs[n:]:x+=(p-x)*2/(n+1);vs.append(x)
    return vs

def atr(cs):
    ts=[max(b['h']-b['l'],abs(b['h']-a['c']),abs(b['l']-a['c'])) for a,b in zip(cs,cs[1:])]
    v=sum(ts[:14])/14
    for t in ts[14:]:v=(v*13+t)/14
    return v
checks=[]
for i,folder in enumerate(folders,1):
    report=ROOT/f'artifacts/pattern-research-2026-11-11/{i:02d}-trend_breakout.json'
    bars={s:[{k:float(v) if k in ['o','h','l','c','v'] else v for k,v in r.items()}for r in json.loads((folder/f'{s}-1h.json').read_text())]for s in ['BTC','ETH','SOL','HYPE']}
    for run in json.loads(report.read_text())['results']:
        count=0
        for order in run['orders']:
            if order['reduce_only'] or not order['fills']:continue
            at=order['time']//3600000*3600000
            assert order['time']==at+1000
            history=[c for c in bars[order['symbol']]if c['T']<at][-100:]
            assert len(history)==100
            xs=[c['c']for c in history];fast=ema(xs,20);slow=ema(xs,50);a=atr(history);last=history[-1];prior=history[-21:-1]
            assert last['v']>=1.5*sum(c['v']for c in prior)/20-1e-8
            assert last['h']-last['l']<=2.5*a+1e-8
            location=(last['c']-last['l'])/(last['h']-last['l'])
            if order['buy']:
                assert fast[-1]>slow[-1]>slow[-6]
                assert last['c']>max(c['h']for c in prior)
                assert last['c']>last['o'] and location>=.75-1e-10
            else:
                assert fast[-1]<slow[-1]<slow[-6]
                assert last['c']<min(c['l']for c in prior)
                assert last['c']<last['o'] and location<=.25+1e-10
            count+=1
        checks.append({'period':i,'spread_bps':run['spread_bps'],'entries_verified':count})
production=json.loads((OUT/'production-before-sha256.json').read_text())
assert all(hashlib.sha256((ROOT/f).read_bytes()).hexdigest()==digest for f,digest in production.items())
result={'status':'pass','checks':checks,'scope':'Benchmark trend_breakout entry features and time causality independently recalculated with floats; accounting and candidate tests are separate','production_unchanged':True}
(OUT/'independent-benchmark-signal-audit.json').write_text(json.dumps(result,indent=2)+'\n')
print('PASS',sum(c['entries_verified']for c in checks),'benchmark entries across18 runs; production unchanged')
