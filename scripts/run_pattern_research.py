#!/usr/bin/env python3
"""Run frozen offline hypotheses, selecting only on first three windows."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess
from decimal import Decimal as D
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'artifacts/pattern-research-2026-11-11'
PLAN=json.loads((OUT/'research-plan.json').read_text())
assert hashlib.sha256((ROOT/PLAN['rules_source']).read_bytes()).hexdigest()==PLAN['rules_sha256']
DATA=sorted((ROOT/'artifacts/prior-30d-2026-11-11').glob('20*'))+[ROOT/'artifacts/backtest-30d-2026-11-11']

def task(job):
    index,folder,pattern=job
    dest=OUT/f'{index+1:02d}-{pattern}.json'
    subprocess.run([str(ROOT/'target/debug/examples/pattern_research'),str(folder),str(dest),'1h',pattern],check=True,cwd=ROOT)
    # The independent verifier resolves source candles alongside its report.
    subprocess.run(['python3',str(ROOT/'scripts/verify_ohlc_report.py'),str(dest),str(folder)],check=True,cwd=ROOT,stdout=subprocess.DEVNULL)
    print('Verified',dest.name,flush=True)
    return dest

def stage(indices):
    jobs=[(n,DATA[n],pattern) for n in indices for pattern in PLAN['candidates']]
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        list(pool.map(task,jobs))

def main():
    stage(range(3))
    ranking=[]
    for pattern in PLAN['candidates']:
        runs=[next(r for r in json.loads((OUT/f'{n+1:02d}-{pattern}.json').read_text())['results'] if r['spread_bps']=='5') for n in range(3)]
        ranking.append({'pattern':pattern,'pnl':str(sum((D(r['final_equity'])-D(r['capital']) for r in runs),D(0))),'closed_trades':sum(r['closed_trades'] for r in runs)})
    eligible=sorted([r for r in ranking if r['closed_trades']>=30],key=lambda r:D(r['pnl']),reverse=True)
    selection={'ranking':ranking,'selected':eligible[0]['pattern'] if eligible else None,'rule':PLAN['selection_rule']}
    (OUT/'development-selection.json').write_text(json.dumps(selection,indent=2)+'\n')
    print('FROZEN SELECTION',json.dumps(selection),flush=True)
    stage(range(3,6))
if __name__=='__main__':main()
