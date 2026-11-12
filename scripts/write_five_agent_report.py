#!/usr/bin/env python3
from pathlib import Path
from decimal import Decimal as D
import json, hashlib
ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/'artifacts/five-agent-research-2026-11-11'
s = json.loads((OUT/'comparison.json').read_text())
text = '''CINCO ANÁLISIS Y DEBATE DE PATRONES — SIN IMPLEMENTACIÓN

Datos: BTC, ETH, SOL y HYPE, velas de una hora. Seis bloques consecutivos de 30 días, desde el 15/05/2026 hasta el 11/11/2026 (excluido). Capital inicial de 10.000 USDC por bloque y escenario.
Participantes: analyst_1_breakouts, analyst_2_reversals, analyst_3_pullbacks, analyst_4_regimes y analyst_5_execution. Trabajaron por tandas por el límite de concurrencia.

CONCLUSIÓN

No se ha identificado una variante nueva validada que permita prometer crecimiento de ganancias. La selección central, fijada con los datos de desarrollo, eligió efficiency_breakout; esta perdió en el contraste posterior. No se sustituye retrospectivamente por otra ganadora.

La ruptura confirmada anterior (trend_breakout) sigue siendo la referencia histórica más prometedora entre las comparadas. Una prueba continua de 180 días conserva beneficio en la aproximación, pero utiliza los mismos datos y no demuestra rentabilidad real ni fuera de muestra. Ninguna regla de producción ni parámetro de riesgo se ha cambiado.

TRABAJO DE LOS CINCO ANALISTAS

1. Rupturas y estructura: compresión previa y canal de 40 horas. Ninguna supera la referencia con muestra adecuada.
2. Reversiones: barrido lateral confirmado y falsa ruptura contra tendencia. Ambas fallan desarrollo; un contraste positivo aislado no rescata una regla perdedora.
3. Continuación: bandera de impulso y pausa eficiente. Solo 2 y 14 cierres de desarrollo, respectivamente: muestra insuficiente. La pausa pierde después.
4. Regímenes: eficiencia direccional y separación de medias. Mejores resultados de desarrollo; ambas pierden en contraste. La primera fue la ganadora CENTRAL y queda rechazada.
5. Ejecución y distancia de entrada: ruptura a no más de 0,5 ATR del nivel. Inferior a la referencia y negativa en contraste. La auditoría no detecta un fallo material de temporalidad, pero liquidez y oráculos siguen siendo supuestos.

PROTOCOLO

Nueve hipótesis nuevas fijadas antes de sus simulaciones, sin búsqueda de combinaciones ni reajustes posteriores. Se ejecutaron 162 escenarios nuevos (9 patrones × 6 periodos × 3 spreads), además de controles y pruebas continuas.

Primeros 90 días para el ranking, con un mínimo de 30 cierres; últimos 90 días para comprobar el candidato congelado. Las métricas generales de todos estos datos ya eran conocidas: NO es una muestra de prueba virgen. Cinco agentes no crean cinco conjuntos de datos independientes.

La regla temporal exigía PnL positivo a 5 y 10 puntos básicos, al menos 30 cierres y dos de tres bloques positivos. El mínimo de operaciones es una regla de investigación, no una prueba de significancia estadística.

RESULTADOS CON CUENTAS INDEPENDIENTES Y SPREAD DE 5 PUNTOS BÁSICOS

Patrón | PnL desarrollo (USDC) | Cierres | PnL contraste (USDC) | Cierres | Muestra elegible
'''
for r in s['candidates']:
    a, b = r['stages']['development'], r['stages']['temporal_check']
    text += f"{r['pattern']} | {D(a['pnl_sum_independent_accounts']):+.2f} | {a['closed_trades']} | {D(b['pnl_sum_independent_accounts']):+.2f} | {b['closed_trades']} | {'Sí' if r['development_eligible'] else 'No'}\n"
text += '''
Estas cifras suman el PnL de tres cuentas nuevas en cada fase. No son rendimiento compuesto ni una cuenta que conserva posiciones.

Ganadora central: efficiency_breakout. Contraste: −49,78 USDC a 5 puntos básicos y −49,69 a 10, con 36 cierres. Dos bloques positivos no compensan el negativo. Resultado del protocolo: FALLA.

QUÉ SE DISCUTIÓ

• Analistas 1 y 2: atribuir ganancias a una estrategia dentro de una cartera no predice su resultado aislado. Una confirmación tardía puede consumir el recorrido disponible; un objetivo fijo puede no encajar con un rango.
• Analistas 1 y 4: concentración por activo y régimen, muestras pequeñas y filtros que pueden eliminar tendencias tempranas. HYPE aporta 199,07 USDC cerrados de la referencia, frente a 109,24 del resto; son 56 operaciones, no una única ganadora, pero sigue habiendo concentración.
• Analistas 1, 4 y 5: limitar la distancia al nivel puede seleccionar rupturas débiles y eliminar expansiones persistentes. Que stop, objetivo y tamaño cambien con la entrada hace que el efecto del spread no sea simplemente restar un coste fijo.
• Analistas 3 y 5: una curva continua mejora la representación económica, pero no aporta datos independientes. Se deben comparar costes terminales de manera coherente y conservar las objeciones sobre tamaño muestral.
• Acuerdo: no retocar umbrales para rescatar resultados, no ocultar el fallo del ganador y no llamar a estos datos una prueba nueva fuera de muestra.
• Pendiente: ejecución real, oráculos históricos, cambios de régimen y sesgo acumulado al investigar múltiples reglas sobre los mismos datos.

PRUEBA COMPLEMENTARIA: UNA SOLA CUENTA DURANTE 180 DÍAS

Conserva capital, posiciones, financiación y estado de riesgo entre bloques. El tamaño de las operaciones sigue limitado por el capital inicial, según las reglas existentes.

Patrón | Spread | Capital final | Retorno de 180 días | Caída máxima aproximada | Cierres | Posiciones abiertas
'''
for r in s['continuous_benchmark']['results']:
    text += f"{r['pattern']} | {r['spread_bps']} bps | {D(r['final_equity']):.2f} USDC | {100*D(r['net_return']):+.2f}% | {100*D(r['sampled_max_drawdown']):.2f}% | {r['closed_trades']} | {r['open_positions']}\n"
text += '''
A 5 puntos básicos, la candidata anterior termina con 10.391,27 USDC (+391,27; +3,91%) y sin posiciones abiertas. El sistema original termina con 9.410,11 USDC (−589,89; −5,90%) y dos posiciones abiertas. El coste estimado de cerrarlas sería otros 0,92 USDC: capital ajustado aproximado de 9.409,18 USDC.

Añadir 5 puntos básicos por operación completa, manteniendo los fills de la candidata previa, reduce su beneficio estimado a unos 316,54 USDC. Es un estrés contable descriptivo, no una garantía de liquidez.

La auditoría no encontró salidas donde stop y objetivo resultaran ambos alcanzables en una misma vela en estas pruebas continuas. Persisten los límites de observar únicamente cuatro puntos modelados por vela y los gaps de apertura.

PATRÓN QUE MERECE SEGUIR EVALUANDO: RUPTURA CONFIRMADA

Es la candidata ANTERIOR, no el filtro de eficiencia que acaba de fallar. No se añaden los nueve filtros o variantes descartados.

Para una compra:
1. EMA20 por encima de EMA50 y EMA50 mayor que cinco velas antes.
2. Cierre por encima del máximo de las 20 velas anteriores, excluyendo la vela de señal.
3. Volumen de señal de al menos 1,5 veces la media de las 20 anteriores.
4. Cierre mayor que apertura y situado en el cuarto superior de la vela.
5. Rango de la vela no superior a 2,5 ATR14, para descartar velas desproporcionadas.
6. Esperar al cierre y entrar en la apertura siguiente según el modelo. Para cortos se invierten dirección, nivel y posición del cierre.

Riesgo intacto: 0,25% por entrada, máximo dos posiciones, exposición del 20%, exposición del mismo sentido del 15% y límite diario del 1%. Stop a 2 ATR con distancia mínima del 0,5%; objetivo a dos veces la distancia del stop.

La idea es exigir que el mercado supere un nivel relevante con volumen y sostenga el avance al cierre, a favor de la tendencia. No basta con un valor de RSI o con que el precio haya cambiado.

Ejemplo ficticio: resistencia en 100 y volumen medio de 1.000. Una vela abre en 99,8, marca mínimo de 99,5 y máximo de 101,2, cierra en 101,0 y negocia 1.600, con ATR de 1. Cumple si las medias confirman. Con apertura siguiente en 101,1, el stop sería aproximadamente 99,1 y el objetivo 105,1, antes de costes y redondeo. Si no se cumplen las condiciones, no se opera.

LÍMITES Y PROPUESTA ANTES DE IMPLEMENTAR

Las velas y tasas de financiación son históricas. La profundidad de un millón de USDC por lado, los spreads, la latencia, el recorrido intravela y el sustituto del oráculo son supuestos. Se analiza solo una hora, no la cartera multitemporal completa.

Propuesta: conservar la ruptura confirmada como candidata de investigación, con reglas congeladas; no implementar las nueve variantes nuevas. Antes de desplegar, revisar estas reglas con el usuario y evaluar datos nuevos con ejecución verificable. No se ha demostrado un patrón que garantice ganancias.

TRAZABILIDAD

• protocol.json y central-development-selection.json: reglas y selección antes del contraste central.
• comparison.json: consolidación recalculada desde los informes, con sus huellas.
• Carpetas agent1 a agent5: planes, informes y debates de cada especialista.
• continuous-benchmark/: datos concatenados sin huecos, informes completos y resumen de 180 días.
• independent-benchmark-signal-audit.json: 435 entradas verificadas independientemente con datos anteriores a la ejecución.
• production-before-sha256.json: huellas para comprobar que las reglas de producción siguen intactas.
'''
(OUT/'informe-conjunto.txt').write_text(text)
files = [OUT/'protocol.json',OUT/'central-development-selection.json',OUT/'comparison.json',OUT/'informe-conjunto.txt']
files += list(OUT.glob('agent*/summary.json')) + list(OUT.glob('agent*/debate.json'))
manifest = {str(f.relative_to(ROOT)):hashlib.sha256(f.read_bytes()).hexdigest() for f in files}
(OUT/'joint-report-sha256.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Saved joint report; production unchanged:',s['production_unchanged'])
