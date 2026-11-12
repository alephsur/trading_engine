#!/usr/bin/env python3
"""Summarize independently reset 30-day experiments without inventing an annual return."""
from decimal import Decimal as D
from pathlib import Path
import hashlib
import json

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'artifacts/prior-30d-2026-11-11'
coverage=json.loads((OUT/'coverage.json').read_text())
reports=[]
for window in coverage['windows']:
    folder=OUT/window['directory']
    for name,digest in window['sha256'].items():
        assert hashlib.sha256((folder/name).read_bytes()).hexdigest()==digest
    reports.append((window['start_utc'][:10],window['end_exclusive_utc'][:10],folder/'report.json',False))
reports.append(('2026-10-12','2026-11-11',ROOT/'artifacts/backtest-30d-2026-11-11/report-verified.json',True))
rows=[]
identity=None
for start,end,path,reference in reports:
    data=json.loads(path.read_text())
    current=(data['strategy_manifest'],data['risk_config'])
    if identity is None:identity=current
    assert identity==current,'configuration changed between periods'
    for result in data['results']:
        if result['interval']!='1h':continue
        rows.append({'start_utc':start,'end_exclusive_utc':end,'previous_reference':reference,
          **{k:result[k] for k in ('interval','spread_bps','capital','final_equity','net_return','sampled_max_drawdown','closed_trades','winning_trades','fees','funding_cash_flow')},
          'open_positions':len(result['open_positions']),
          'source_report':str(path.relative_to(ROOT)),
          'report_sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
summary={'model':'ohlc_approx_v1_NOT_RECORDED_REPLAY','independent_accounts':True,'combined_return':None,
'combined_return_reason':'Each 30-day window resets capital, positions and risk state; not a continuous six-month simulation.',
'source_sha256':hashlib.sha256((ROOT/'crates/backtest/examples/ohlc_approx.rs').read_bytes()).hexdigest(),
'rows':rows}
(OUT/'comparison.json').write_text(json.dumps(summary,indent=2)+'\n')
lines=['COMPARACIÓN DE PERIODOS DE 30 DÍAS — SIMULACIÓN APROXIMADA',
'BTC, ETH, SOL y HYPE. Velas de 1 hora. Capital inicial independiente: 10.000 USDC por periodo y escenario.',
'Fechas UTC; el extremo final indicado queda excluido. Cinco periodos nuevos y el periodo anterior como referencia.',
'', 'ESCENARIO DE REFERENCIA: SPREAD DE 5 BPS',
'Inicio | Fin excluido | Equity final (USDC) | Retorno neto | Caída máxima estimada | Cierres | Aciertos']
for r in rows:
    if r['spread_bps']=='5':
        lines.append(f"{r['start_utc']} | {r['end_exclusive_utc']} | {D(r['final_equity']):.2f} | {100*D(r['net_return']):+.2f}% | {100*D(r['sampled_max_drawdown']):.2f}% | {r['closed_trades']} | {100*r['winning_trades']/r['closed_trades']:.2f}%")
lines+=['','SENSIBILIDAD AL SPREAD','Inicio | 2 bps | 5 bps | 10 bps']
for start,end,_,_ in reports:
    group={r['spread_bps']:r for r in rows if r['start_utc']==start}
    lines.append(start+' | '+' | '.join(f"{100*D(group[s]['net_return']):+.2f}%" for s in ['2','5','10']))
lines+=['','SUPUESTOS Y LÍMITES',
'- Mismo código de estrategias y riesgo, sin ajustar parámetros a estos resultados. Cuenta nueva al comenzar cada bloque; no se concatenan ni anualizan los porcentajes.',
'- Comisión de 4,5 bps y slippage adverso de 2 bps por lado; spreads supuestos de 2, 5 y 10 bps. Liquidez sintética de 1 millón de USDC por lado.',
'- Entrada a la apertura siguiente a la señal, con latencia nominal de un segundo. Historial de calentamiento de 100 velas. Se omite la señal anterior a la activación.',
'- Recorrido intravela supuesto: apertura, extremo adverso a la posición, extremo favorable, cierre. Prioridad del stop si ambos umbrales se alcanzan. Gaps a precio de apertura.',
'- Tasas históricas de financiación reales; apertura horaria como sustituto del oráculo histórico. Metadatos actuales para la precisión de precios y cantidades.',
'- Equity incluye posiciones abiertas a bid/ask supuesto; no descuenta sus posibles comisiones ni slippage de cierre. Caída máxima muestreada en puntos simulados.',
'- Solo intervalo de 1 hora; no representa la cartera conjunta de todos los intervalos del sistema. No hay histórico de 15 minutos suficiente para un bloque anterior completo no solapado.',
'- Cobertura completa por periodo/activo: 720 velas + 100 de calentamiento y 720 tasas horarias. Precios y financiación descargados de https://api.hyperliquid.xyz/info.',
'- Comprobación independiente de fills, comisiones, financiación, cantidades, cierres y equity en cada escenario. El nuevo selector de intervalo reproduce exactamente el resultado horario previo.',
'', 'ARCHIVOS',
'coverage.json: cobertura y huellas de datos originales.',
'comparison.json: resultados resumidos y huellas de informes.',
'Cada carpeta de fechas conserva datos originales e informe detallado report.json.',
'', 'REPRODUCCIÓN',
'python3 scripts/download_prior_30d.py (solo en un directorio de salida aún no generado)',
'cargo run --locked -p trading-backtest --example ohlc_approx -- CARPETA_DATOS NUEVO_INFORME.json 1h',
'python3 scripts/verify_ohlc_report.py CARPETA_DATOS/report.json',
'python3 scripts/summarize_prior_30d.py']
(OUT/'comparativa.txt').write_text('\n'.join(lines)+'\n')
print('\n'.join(lines[:20]))
