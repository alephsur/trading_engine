#!/usr/bin/env python3
"""Descriptive attribution of existing simulated trades; does not estimate filtered portfolios."""
import collections
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'artifacts/pattern-research-2026-11-11'
FILES=sorted((ROOT/'artifacts/prior-30d-2026-11-11').glob('*/report.json'))+[ROOT/'artifacts/backtest-30d-2026-11-11/report-verified.json']

def ema(xs,n):
    values=[None]*(n-1);s=sum(xs[:n])/n;values.append(s)
    for x in xs[n:]:s+=(x-s)*2/(n+1);values.append(s)
    return values

def atr(cs):
    tr=[max(b['h']-b['l'],abs(b['h']-a['c']),abs(b['l']-a['c'])) for a,b in zip(cs,cs[1:])]
    a=sum(tr[:14])/14
    for t in tr[14:]:a=(a*13+t)/14
    return a

def stats(rows):
    vs=[r['net'] for r in rows];wins=[v for v in vs if v>0];losses=[v for v in vs if v<0]
    avgwin=sum(wins)/len(wins) if wins else 0;avgloss=-sum(losses)/len(losses) if losses else 0
    return {'count':len(vs),'net_pnl':sum(vs),'gross_pnl':sum(r['gross'] for r in rows),'fees':sum(r['fees'] for r in rows),'funding':sum(r['funding'] for r in rows), 'wins':len(wins),'win_rate':len(wins)/len(vs) if vs else None,'profit_factor':sum(wins)/-sum(losses) if losses else None,'average_win':avgwin,'average_loss':avgloss,'breakeven_win_rate_from_observed_payoff':avgloss/(avgwin+avgloss) if avgwin+avgloss else None,'average_holding_hours':sum(r['hours'] for r in rows)/len(rows) if rows else None}

rows=[];rejections=collections.Counter();equity_delta=0;allfees=allfunding=0
for idx,path in enumerate(FILES):
    r=next(v for v in json.loads(path.read_text())['results'] if v['interval']=='1h' and v['spread_bps']=='5')
    rejections.update(r['rejections']);equity_delta+=float(r['final_equity'])-float(r['capital']);allfees+=float(r['fees']);allfunding+=float(r['funding_cash_flow'])
    bars={s:[{k:float(v) if k in ['o','h','l','c','v'] else v for k,v in b.items()} for b in json.loads((path.parent/f'{s}-1h.json').read_text())] for s in ['BTC','ETH','SOL','HYPE']}
    orders=collections.defaultdict(list);funds=collections.Counter()
    for order in r['orders']:orders[order['position_id']].append(order)
    for f in r['funding']:funds[f['position_id']]+=float(f['cash_change'])
    for tid,tr in r['trades'].items():
        if not tr['closed']:continue
        os=orders[int(tid)];entry=next(o for o in os if not o['reduce_only']);exit=next(o for o in os if o['reduce_only']);long=entry['buy'];sign=1 if long else -1
        at=entry['time']//3600000*3600000
        history=[b for b in bars[tr['symbol']] if b['t']<at][-100:]
        assert len(history)==100
        closes=[b['c'] for b in history];fast=ema(closes,20);slow=ema(closes,50);a=atr(history)
        change=abs(closes[-1]-closes[-21]);travel=sum(abs(b-a) for a,b in zip(closes[-21:-1],closes[-20:]));eff=change/travel if travel else 0
        fees=sum(float(f['fee']) for o in os for f in o['fills']);fund=funds[int(tid)];net=float(tr['net_realized']);gross=net+fees-fund
        samebar=entry['time']//3600000==exit['time']//3600000
        touched_both=any(b['l']*(1-.00025)<=float(entry['stop']) and b['h']*(1-.00025)>=float(entry['take_profit']) if long else b['h']*(1+.00025)>=float(entry['stop']) and b['l']*(1+.00025)<=float(entry['take_profit']) for b in bars[tr['symbol']] if at<=b['t']<=exit['time']//3600000*3600000)
        rows.append({'period':idx+1,'strategy':tr['strategy'],'symbol':tr['symbol'],'side':'long' if long else 'short','net':net,'gross':gross,'fees':fees,'funding':fund,'hours':(exit['time']-entry['time'])/3600000,'exit':exit['reason'],'entry_hour':at,'efficiency':eff,'efficiency_bucket':'<0.25' if eff<.25 else '0.25-0.5' if eff<.5 else '>=0.5','trend_aligned':sign*(fast[-1]-slow[-1])>0 and sign*(slow[-1]-slow[-6])>0,'extension_atr':sign*(closes[-1]-fast[-1])/a,'same_bar_exit':samebar,'ambiguous_ohlc':touched_both})
output={'description':'Closed-trade attribution, not the performance of removing strategies; independent period resets. Features measured before entry. Floats for diagnostics only.','totals':stats(rows),'sum_independent_equity_changes':equity_delta,'all_fees':allfees,'all_funding':allfunding,'rejections':rejections,'groups':{},'rows':rows}
for axis in ['strategy','symbol','side','efficiency_bucket','trend_aligned','exit','same_bar_exit','ambiguous_ohlc']:
    groups=collections.defaultdict(list)
    for r in rows:groups[str(r[axis])].append(r)
    output['groups'][axis]={k:stats(v) for k,v in groups.items()}
output['strategy_by_regime']={}
for strategy in sorted({r['strategy'] for r in rows}):
    output['strategy_by_regime'][strategy]={str(align):stats([r for r in rows if r['strategy']==strategy and r['trend_aligned']==align]) for align in [True,False]}
(OUT/'baseline-diagnostics.json').write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps({k:v for k,v in output.items() if k not in ['rows','groups','strategy_by_regime']},indent=2))
print('STRATEGY BY REGIME',json.dumps(output['strategy_by_regime'],indent=2))
