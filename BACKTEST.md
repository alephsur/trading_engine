# Backtesting y validación temporal — fase 5

`trading-backtest` ejecuta experimentos offline sobre datos registrados. Usa los
mismos `trading-quant::Engine` y `trading-paper::Account` de PAPER, incluyendo riesgo,
precisión, spreads, slippage, comisiones, liquidez, stops y financiación. No abre
conexiones de mercado, firma órdenes ni modifica la cuenta PAPER durante `run`.
El servidor continúa admitiendo únicamente `TRADING_MODE=PAPER`; el backtest es un
programa separado y no un modo que habilite ejecución en el servidor.

## Ejecutar un experimento

1. Captura datos con el servidor. Arráncalo con esta versión para aplicar las
   migraciones, incluida la hora de disponibilidad de tasas de financiación.
2. Elige un intervalo **pasado** para el que existan libros, oráculos y velas
   confirmadas. Exporta desde la DB; START_MS/END_MS son milisegundos UTC:

```sh
DATABASE_URL='postgres://trading:local_only@localhost:5433/trading' \
  cargo run --locked -p trading-backtest -- export START_MS END_MS dataset.json
```

3. Copia [config/backtest.confirmed-breakout.example.json](config/backtest.confirmed-breakout.example.json), sustituye las
   cuatro fronteras por fechas dentro del dataset y fija los candidatos **antes**
   de observar validación/prueba. Los valores de fecha del ejemplo son ilustrativos.
4. Ejecuta sin necesidad de PostgreSQL ni Internet:

```sh
cargo run --locked -p trading-backtest -- run dataset.json config/backtest.confirmed-breakout.example.json report.json
```

Este ejemplo usa la estrategia PAPER aprobada y el intervalo 1h. El motor rechaza
combinar ese perfil con otros intervalos. `config/backtest.example.json` conserva
el experimento histórico de cuatro estrategias. Cambiar el default del servidor
no cambia los candidatos declarados explícitamente en un backtest anterior.

Los archivos de salida deben ser nuevos: no se sobrescriben experimentos previos.
El informe JSON contiene datos de procedencia, huella SHA-256 del dataset completo,
configuración, identificador reproducible de ejecución, candidatos de entrenamiento,
selección, validación, prueba, curvas, propuestas, decisiones, órdenes/fills, trades,
funding y estado final de cada cuenta. Datos/configuración iguales producen el mismo
informe byte por byte. No se incluyen hora de ejecución, claves ni conexión a la DB.
El algoritmo se identifica como `backtest-v1`; cambios de semántica requieren versionarlo.

Prueba reproducible independiente de servicios externos:

```sh
python3 scripts/backtest_smoke.py
```

Genera datos **sintéticos**, ejecuta el CLI, compara informes y modifica únicamente
el periodo fuera de muestra para comprobar el aislamiento del entrenamiento. Sus
resultados no son una demostración de rentabilidad ni una reconstrucción del mercado.

## Datos y reloj

El formato JSON tiene `version:1`, `source`, `capture_start`, `capture_end` y una
lista `records`, ordenada por `available_at`. Cada registro contiene un `event`
con `kind` igual a `metadata`, `candle`, `book`, `oracle` o `funding`. Las estructuras
están en `crates/backtest/src/data.rs`; los decimales se codifican como strings.
Los metadatos deben identificar los cuatro activos e índices únicos. Se rechazan
velas no cerradas, libros futuros respecto a disponibilidad, revisiones/duplicados
de datos y secuencias temporales incorrectas.

El exportador abre una transacción PostgreSQL de solo lectura y snapshot consistente.
Conserva recepción/finalización de velas y libros, observación de oráculos y recepción
de tasas. Incluye hasta 500 velas anteriores por serie para warmup y los metadatos
conocidos antes de la captura. No descarga históricos ni reconstruye libros perdidos.
Las tasas existentes al aplicar la migración 0005 reciben la hora de migración: no
se atribuye retrospectivamente una hora de disponibilidad que nunca se registró.
Los timestamps de recepción describen observación/escritura, no la confirmación exacta
de cada transacción original. Un libro con reloj adelantado se programa como mínimo
a su timestamp de exchange. La hora del sistema debe estar sincronizada.

Se avanza un reloj de 1 segundo. En cada paso se incorporan solo registros ya
disponibles; se evalúa el último cierre observable de cada activo para el intervalo
seleccionado, con ventana completa y contigua. Los cierres intermedios de un lote
histórico recibido a la vez no se convierten en entradas retrospectivas. Las señales
esperan una latencia configurable de 1–60 segundos; se ejecutan en el primer tick
posterior elegible usando el libro disponible entonces. No se ejecutan contra el
precio de cierre de la vela que creó la señal. Se mantienen los límites de edad y
frescura de PAPER, incluida la prohibición de reutilizar liquidez de un snapshot.

La preparación exige los cuatro activos, historial suficiente y último cierre de
edad no superior a un intervalo más 120 s. Los huecos impiden nuevas entradas; los
stops usan únicamente libros recibidos. No se supone un recorrido OHLC intrabar:
un cruce que no aparece en los snapshots puede no observarse. Tampoco se reconstruye
el estado exacto de transporte ni el instante de cada desconexión del proceso original.
Es replay de observaciones con un reloj definido, no reproducción exacta del scheduler.

## Separación temporal

Las fronteras deben cumplir `train_start < validation_start < test_start < end`,
alineadas a segundos y dentro de la captura. Los intervalos son semiabiertos:
entrenamiento `[train_start, validation_start)`, validación `[validation_start,
test_start)` y prueba `[test_start, end)`. No se procesa el tick del extremo derecho.
Cada tramo empieza con capital y posiciones nuevos; puede usar velas anteriores ya
conocidas como warmup, pero no ejecutar señales cuyo cierre preceda al tramo.
No se transfieren posiciones, resultados, límites diarios ni órdenes pendientes.

Se admiten de uno a ocho candidatos de estrategia declarados de antemano, con un
único perfil de riesgo y costes para todos. Se selecciona la mayor rentabilidad neta
final de entrenamiento entre candidatos con datos preparados en todos sus ticks,
valoración final conocida y al menos `minimum_training_trades` cierres liquidados
(30 en el ejemplo; rango permitido 1–1000). Los empates respetan el orden declarado.
La elección queda fija para validación y prueba; sus resultados no participan en
el ranking. Una financiación temporalmente pendiente puede invalidar métricas de
trayectoria, aunque una liquidación posterior permita valorar el final.

Si ningún candidato cumple, `selection_status` indica
`no_eligible_training_candidate` y no se ejecutan los tramos reservados. No se fuerza
un ganador sin datos suficientes. Una selección con rentabilidad negativa sigue
siendo negativa: no hay umbral que la convierta en una recomendación de uso.
Repetir manualmente búsquedas después de ver el test puede contaminarlo; conservar
huellas y resultados permite auditar experimentos, no impedir decisiones externas.

## Contabilidad y métricas

- Equity: efectivo más PnL abierto por bid/ask de liquidación. Las posiciones abiertas
  al final siguen abiertas y visibles; no hay cierre gratuito o ficticio. No incluye
  hipotéticos costes de salida de esas posiciones. Rentabilidad neta = equity final /
  capital inicial − 1, tras costes ya incurridos. Valoración final desconocida produce `null`.
- Drawdown: máximo descenso relativo desde el máximo anterior de equity, incluyendo
  el capital inicial. Se calcula **cada segundo**, aunque la curva publicada toma
  muestras cada minuto y en el último tick. Si algún tick no puede valorarse,
  `max_drawdown` es `null`; `observed_max_drawdown` conserva el máximo parcial observado.
- Trade: posición identificada por su señal, agrupando salidas parciales. PnL neto
  realizado = PnL bruto realizado − fees + flujo de funding. Una posición cerrada
  con funding pendiente aún no cuenta como trade liquidado. Las posiciones abiertas
  muestran lo realizado hasta ese momento, separado de su valoración abierta.
- Win rate: cierres con PnL neto positivo / cierres liquidados, incluyendo breakeven
  en el denominador. Profit factor: ganancias netas / valor absoluto de pérdidas
  netas de esos cierres. Se publican solo con 30 cierres liquidados y sin obligaciones
  de funding pendientes en los trades. Sin pérdidas el profit factor es `null`,
  no infinito. Se muestran recuentos incluso cuando los ratios no están disponibles.
- Sharpe: media / desviación típica **muestral** de rendimientos simples diarios,
  muestreados cada 24 horas desde el inicio del tramo, con tipo libre de riesgo cero.
  Requiere 30 rendimientos diarios completos, valoración continua y equity positiva.
  Varianza cero produce `null`. La estadística usa `f64`; la contabilidad usa Decimal.
  `sharpe_annualized_iid = sharpe_daily × sqrt(365)` declara expresamente la hipótesis
  de rendimientos independientes e idénticamente distribuidos. No es un intervalo
  de confianza ni una corrección por autocorrelación. La definición sigue a
  [William F. Sharpe](https://web.stanford.edu/~wfsharpe/art/sr/sr.htm).

Los 30 trades/días son umbrales mínimos de presentación, no garantía de significancia.
Los campos de estado explican `null`; nunca se sustituye una métrica desconocida por cero.
Las tasas y el oráculo siguen [el modelo PAPER](PAPER.md): una obligación sin datos
no desaparece y bloquea entradas. No se usa una tasa publicada en el futuro para
decidir una entrada pasada. Una muestra con financiación pendiente en un tick no
produce Sharpe ni drawdown completo, aunque luego se liquide el importe.

## Límites de esta entrega

El replay carga el dataset en memoria: hasta dos millones de registros, entradas
JSON de hasta 512 MiB y tres millones de ticks (unos 34,7 días por experimento).
El exportador limita la ventana a 35 días; reduce la ventana si excede registros.
La captura densa de libros consume espacio rápidamente. Un dataset pequeño puede
ser suficiente para probar el funcionamiento, pero no para publicar ratios de muestra.
La captura PAPER anterior no garantiza funding para todas las estrategias hipotéticas:
solo se exportan tasas realmente guardadas. Faltas de cobertura quedan visibles.
No hay optimización automática de hiperparámetros, walk-forward repetido, reconstrucción
OHLC de liquidez ni afirmación de rentabilidad. Dashboard corresponde a la fase 6.
