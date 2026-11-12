#!/usr/bin/env python3
"""Consolidate frozen agent experiments without changing their selection retrospectively."""
from pathlib import Path
from decimal import Decimal as D
import hashlib,json
ROOT=Path(__file__).resolve().parents[1];OUT=ROOT/'artifacts/five-agent-research-2026-11-11'
selection=json.loads((OUT/'central-development-selection.json').read_text())
risk=json.loads((ROOT/'config/paper.example.json').read_text())
reports={};files={}
for n in range(1,6):
    for path in (OUT/f'agent{n}').glob('*.json'):
        obj=json.loads(path.read_text())
        if not isinstance(obj,dict) or 'results' not in obj or 'risk_config' not in obj:continue
        assert obj['risk_config']==risk
        pattern=obj['research_pattern']
        for run in obj['results']:
            key=(pattern,run['start'],run['spread_bps'])
            if key in reports:assert reports[key]==run
            reports[key]=run;files[key]=path
rows=[]
for ranked in selection['ranking']:
    pattern=ranked['pattern'];runs=sorted([r for (p,t,s),r in reports.items() if p==pattern and s=='5'],key=lambda r:r['start'])
    assert len(runs)==6,(pattern,len(runs))
    stages={}
    for label,subset in [('development',runs[:3]),('temporal_check',runs[3:])]:
        closed=[t for r in subset for t in r['trades'].values() if t['closed']]
        wins=sum(D(t['net_realized'])>0 for t in closed)
        grosswin=sum((D(t['net_realized'])for t in closed if D(t['net_realized'])>0),D(0));grossloss=-sum((D(t['net_realized'])for t in closed if D(t['net_realized'])<0),D(0))
        stages[label]={'pnl_sum_independent_accounts':str(sum((D(r['final_equity'])-D(r['capital']) for r in subset),D(0))),'closed_trades':sum(r['closed_trades'] for r in subset),'positive_periods':sum(D(r['net_return'])>0 for r in subset),'worst_sampled_drawdown':str(max(D(r['sampled_max_drawdown'])for r in subset)),'win_rate':wins/len(closed)if closed else None,'profit_factor':str(grosswin/grossloss)if grossloss else None,'pnl_sum_10bps':str(sum((D(reports[(pattern,r['start'],'10')]['final_equity'])-D(r['capital'])for r in subset),D(0)))}
    periods=[]
    for r in runs:
        path=files[(pattern,r['start'],'5')]
        periods.append({'start':r['start'],'end_exclusive':r['end_exclusive'],'return_5bps':r['net_return'],'return_10bps':reports[(pattern,r['start'],'10')]['net_return'],'closed_trades':r['closed_trades'],'report':str(path.relative_to(ROOT)),'report_sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
    rows.append({'agent':ranked['agent'],'pattern':pattern,'development_eligible':ranked['eligible'],'stages':stages,'periods':periods})
chosen=next(r for r in rows if r['pattern']==selection['selected']['pattern']);later=chosen['stages']['temporal_check']
gates={'positive_pnl_5bps':D(later['pnl_sum_independent_accounts'])>0,'positive_pnl_10bps':D(later['pnl_sum_10bps'])>0,'minimum_30_closes':later['closed_trades']>=30,'minimum_2_positive_periods':later['positive_periods']>=2}
output={'selected_before_temporal':chosen['pattern'],'temporal_pass':all(gates.values()),'temporal_gates':gates,'conclusion':'Frozen new winner failed; do not replace with another post-hoc winner. Prior trend_breakout remains a separately identified research benchmark, not a newly validated winner.','candidates':rows,'continuous_benchmark':json.loads((OUT/'continuous-benchmark/summary.json').read_text()),'production_unchanged':all(hashlib.sha256((ROOT/f).read_bytes()).hexdigest()==h for f,h in json.loads((OUT/'production-before-sha256.json').read_text()).items())}
(OUT/'comparison.json').write_text(json.dumps(output,indent=2)+'\n')
print('Selected',chosen['pattern'],'PASS',output['temporal_pass'],'gates',gates)
for r in rows:
 a=r['stages']['development'];b=r['stages']['temporal_check'];print(r['pattern'],round(float(a['pnl_sum_independent_accounts']),2),a['closed_trades'],round(float(b['pnl_sum_independent_accounts']),2),b['closed_trades'])
