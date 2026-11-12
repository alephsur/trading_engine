#!/usr/bin/env python3
"""Evidence and proposal only; never changes production strategy/risk configuration."""
import hashlib
import json
from decimal import Decimal as D
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];OUT=ROOT/'artifacts/pattern-research-2026-11-11'
plan=json.loads((OUT/'research-plan.json').read_text());selection=json.loads((OUT/'development-selection.json').read_text());diag=json.loads((OUT/'baseline-diagnostics.json').read_text())
assert hashlib.sha256((ROOT/plan['rules_source']).read_bytes()).hexdigest()==plan['rules_sha256']
basefiles=sorted((ROOT/'artifacts/prior-30d-2026-11-11').glob('*/report.json'))+[ROOT/'artifacts/backtest-30d-2026-11-11/report-verified.json']
def run(path,spread='5'):
    return next(r for r in json.loads(path.read_text())['results'] if r['interval']=='1h' and r['spread_bps']==spread)
def closed(r):return [t for t in r['trades'].values() if t['closed']]
def pf(trades):
    values=[D(t['net_realized']) for t in trades];g=sum((v for v in values if v>0),D(0));l=-sum((v for v in values if v<0),D(0));return str(g/l) if l else None
candidates=[]
for pattern in plan['candidates']:
    periods=[run(OUT/f'{i:02d}-{pattern}.json')for i in range(1,7)]
    stage=[]
    for name,subset in [('development',periods[:3]),('temporal_check',periods[3:])]:
        trades=[t for r in subset for t in closed(r)]
        stage.append({'stage':name,'sum_pnl_independent_accounts':str(sum((D(r['final_equity'])-D(r['capital']) for r in subset),D(0))),'closed_trades':len(trades),'profit_factor_closed':pf(trades),'positive_periods':sum(D(r['net_return'])>0 for r in subset)})
    candidates.append({'pattern':pattern,'stages':stage})
comparison=[]
for i,file in enumerate(basefiles,1):
    b=run(file);p=run(OUT/f'{i:02d}-trend_breakout.json');reverse=run(OUT/f'{i:02d}-reverse-priority.json')
    comparison.append({'period':i,'start':b['start'],'end_exclusive':b['end_exclusive'],'baseline_return':b['net_return'],'candidate_return':p['net_return'],'candidate_drawdown':p['sampled_max_drawdown'],'closed_trades':p['closed_trades'],'open_positions':len(p['open_positions']),'reverse_priority_return':reverse['net_return'],'candidate_spread_10bps_return':run(OUT/f'{i:02d}-trend_breakout.json','10')['net_return']})
summary={'production_changes_applied':False,'selected':selection['selected'],'prior_exposure':plan['prior_exposure'],'selection':selection,'candidates':candidates,'comparison':comparison,'baseline_closed_trades':diag['totals'],'limitations':['No pristine out-of-sample test: past baseline results already observed','Fixed historical datasets from six independent 30-day accounts, not continuous wealth','Synthetic liquidity and intrabar path; historical rate with proxy oracle','Four hypotheses compared; selection bias remains; no profitability guarantee','1h only; full multi-timeframe system not tested']}
(OUT/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
lines=['ANÁLISIS DEL SISTEMA Y PROPUESTA DE PATRÓN — NO APLICADO',
'Periodo estudiado: 15/05/2026–10/11/2026, seis bloques independientes de 30 días. BTC, ETH, SOL y HYPE, velas de 1 hora.',
'Cada bloque comienza con 10.000 USDC. Spread de referencia: 5 bps; taker 4,5 bps y slippage 2 bps por lado.',
'', 'CONCLUSIÓN',
'La candidata con mejor evidencia es una ruptura con volumen, tendencia y cierre fuerte. Mejora la aproximación de referencia, pero no se ha demostrado que sea rentable en operación real.',
'Se conservan sin cambios las estrategias y los límites de riesgo del sistema. Solo se crearon experimentos offline separados.',
'', 'DIAGNÓSTICO',
'366 operaciones cerradas: 112 ganadoras (30,60%), pérdida neta 599,86 USDC; profit factor 0,817.',
'PnL de cierres antes de comisiones/funding: -334,84 USDC. Comisiones: 273,72; funding: +8,70.',
'Ganancia media neta: 23,97 USDC; pérdida media neta: 12,93. Con ese pago medio habría hecho falta ~35,04% de aciertos. Es una descripción de esta muestra, no un umbral universal.',
'La suma de cambios de equity de las seis cuentas es -543,44 USDC, distinta del PnL cerrado porque incluye posiciones abiertas. No se presenta como retorno continuo.',
'', 'Estrategia | Cierres | PnL neto cerrado | Profit factor']
for name,g in diag['groups']['strategy'].items():lines.append(f"{name} | {g['count']} | {g['net_pnl']:+.2f} USDC | {g['profit_factor']:.3f}")
lines+=['',
'- Momentum/RSI se activa por cambio en cinco velas y una banda RSI; no exige ruptura de estructura, pendiente de tendencia ni una vela de confirmación. Fue responsable de 214 cierres y -563,46 USDC. No basta con que el precio se esté moviendo.',
'- Reversión a la media compra debilidad o vende fortaleza sin exigir que el precio haya girado. Sus 70 cierres perdieron 209,70 USDC. El mecanismo puede entrar contra una tendencia persistente; no se atribuye causalidad exclusiva con estos datos.',
'- Los cruces EMA ganaron 79,17 USDC, pero solo hubo 17 cierres. Esa muestra es insuficiente para declarar que sean una solución.',
'- Las rupturas de la cartera original ganaron 94,13 USDC, pero al ejecutarlas solas el resultado empeoró. La atribución de los trades aceptados no equivale a simular una estrategia aislada.',
'- Hay competencia por dos plazas: 9.169 propuestas fueron rechazadas por posición/límite. La aproximación procesa BTC, ETH, SOL y HYPE en ese orden; el servidor real consume por event_id, sin un ranking de calidad. Por eso se ensayó invertir la prioridad simultánea.',
'- BTC y ETH concentran las pérdidas originales, pero no se han eliminado activos a posteriori. La contribución por activo cambia al cambiar las reglas; escoger solo ganadores observados sería sesgado.',
'', 'CUATRO HIPÓTESIS FIJADAS ANTES DE EJECUTARLAS',
'1. Solo ruptura original: máximo/mínimo de 20 velas con volumen 1,5x.',
'2. Ruptura filtrada: añade tendencia, cierre fuerte y límite de tamaño de vela.',
'3. Retesteo: ruptura 2–6 velas antes, regreso al nivel dentro de 0,25 ATR y confirmación; misma tendencia.',
'4. Retroceso en tendencia: toque de EMA20 en tres velas, sin perder EMA50, y recuperación del máximo/mínimo previo con cierre fuerte.',
'No se hizo una búsqueda de combinaciones de umbrales. Se seleccionó por PnL de los tres primeros periodos a 5 bps, exigiendo al menos 30 cierres. El ganador se fijó antes de ejecutar el contraste posterior.',
'Los agregados del sistema original de todos los meses ya eran conocidos. El contraste es temporal, no una muestra completamente virgen.',
'', 'Hipótesis | PnL desarrollo (3 cuentas) | PnL contraste (3 cuentas) | Cierres desarrollo / contraste']
for c in candidates:
    a,b=c['stages'];lines.append(f"{c['pattern']} | {D(a['sum_pnl_independent_accounts']):+.2f} USDC | {D(b['sum_pnl_independent_accounts']):+.2f} USDC | {a['closed_trades']} / {b['closed_trades']}")
lines+=['', 'PATRÓN PROPUESTO: RUPTURA CONFIRMADA EN TENDENCIA',
'Para largos, todas las condiciones deben cumplirse en la misma vela cerrada:',
'1. EMA20 > EMA50 y EMA50 actual > EMA50 de cinco velas antes. Medias calculadas sobre la ventana de 100 velas del sistema.',
'2. Cierre por encima del máximo de las 20 velas anteriores. La vela de señal no cuenta dentro de ese máximo.',
'3. Volumen de la vela >= 1,5 veces el volumen medio de las 20 anteriores.',
'4. Cierre > apertura y cierre dentro del cuarto superior de la vela: (cierre-minimo)/(maximo-minimo) >= 0,75.',
'5. Rango máximo-mínimo de la vela <= 2,5 ATR14, para descartar impulsos excesivamente amplios.',
'6. Entrada en la siguiente apertura según el modelo de ejecución. Si no se cumplen las condiciones, no se entra.',
'Para cortos se invierte: EMA20 < EMA50 descendente, ruptura del mínimo, vela bajista y cierre en su cuarto inferior.',
'Riesgo intacto: stop a 2 ATR, con distancia mínima del 0,5%; objetivo a 2 veces la distancia del stop, ajustado por el modelo existente. Riesgo por entrada 0,25%, máximo dos posiciones y límites de exposición previos.',
'', 'EJEMPLO DIDÁCTICO, NO OPERACIÓN REAL',
'Resistencia previa 100; medias 20/50 en 98/95, con EMA50 ascendiendo. Una vela abre en 99,8, marca mínimo 99,5, máximo 101,2 y cierra en 101,0; volumen 1.600 frente a media 1.000; ATR=1.',
'El cierre supera 100, ocupa el 88% del rango y la vela mide 1,7 ATR: cumple. Con siguiente apertura en 101,1, el stop quedaría aproximadamente en 99,1 y el objetivo en 105,1, antes de ajustes por spread, redondeo y costes.',
'No se compra solo porque haya subido: se exige ruptura, participación y mantenimiento del precio cerca del extremo al cierre. No se exige retroceso posterior: esa variante no ganó la selección.',
'', 'COMPARACIÓN POR PERIODO',
'Bloque | Base | Candidata | Candidata con 10 bps | Prioridad inversa (5 bps) | Caída máx. candidata']
for r in comparison:lines.append(f"{r['period']} | {100*D(r['baseline_return']):+.2f}% | {100*D(r['candidate_return']):+.2f}% | {100*D(r['candidate_spread_10bps_return']):+.2f}% | {100*D(r['reverse_priority_return']):+.2f}% | {100*D(r['candidate_drawdown']):.2f}%")
lines+=['Los bloques 1–3 son desarrollo; 4–6 son contraste temporal. No se suman los porcentajes como rentabilidad acumulada.',
'La candidata cierra 140 trades en seis bloques (55 ganadores, 39,29%); profit factor 1,187. En los tres últimos: 66 cierres, 42,42% aciertos y profit factor 1,304. La evidencia sigue siendo limitada.',
'El bloque de julio–agosto permanece negativo. El bloque septiembre–octubre apenas cubre costes. Rentabilidad y riesgo siguen dependiendo de hipótesis de ejecución.',
'', 'VERIFICACIÓN Y SIGUIENTE PASO',
'La investigación reproduce exactamente las tres variantes horarias originales al seleccionar baseline. Se verificó de forma independiente la contabilidad de los 72 escenarios de hipótesis y los 18 de prioridad inversa.',
'Pruebas del patrón: requisito de volumen, cierre fuerte y simetría largo/corto. Compilación y lint superados.',
'Antes de aplicarlo: revisión del usuario de estas reglas. Después de aprobar, integrar como estrategia versionada y medir datos nuevos sin reajustar parámetros; preferiblemente con libros y oráculos registrados. No se ha aplicado ni activado esta candidata.',
'', 'FUENTES Y TRAZABILIDAD',
'Datos: archivos históricos Hyperliquid ya descargados; hashes en coverage.json y manifiestos previos.',
'Plan fijado: research-plan.json; selección: development-selection.json; prueba de orden: robustness-plan.json.',
'Diagnóstico: baseline-diagnostics.json; comparación: summary.json; informes completos: NN-PATRÓN.json.',
'Sobre el riesgo de seleccionar por resultados históricos: Bailey et al., The Probability of Backtest Overfitting, https://www.davidhbailey.com/dhbpapers/backtest-prob.pdf',
'Los costes, oráculos y liquidez siguen el modelo aproximado descrito en cada informe; esto no acredita rentabilidad real ni conjunta de todos los intervalos del sistema.']
(OUT/'analisis-y-patron.txt').write_text('\n'.join(lines)+'\n')
files=list(OUT.glob('*.json'))+[ROOT/'crates/backtest/examples/pattern_research.rs',ROOT/'crates/backtest/examples/pattern_order_stress.rs',ROOT/plan['rules_source']]
(OUT/'manifest-sha256.json').write_text(json.dumps({str(f.relative_to(ROOT)):hashlib.sha256(f.read_bytes()).hexdigest() for f in files if f.name!='manifest-sha256.json'},indent=2)+'\n')
print(json.dumps({'selected':selection['selected'],'candidates':candidates,'comparison':comparison},indent=2))
