# Riesgo y ejecución PAPER — fase 4

La simulación consume propuestas de la configuración activa y datos públicos reales.
No hay cliente de órdenes del exchange, claves ni firma. `execution_enabled:false`
en salud se refiere a ejecución **real**; `paper_execution_enabled:true` identifica
el simulador. Las señales originales conservan `analysis_only:true`: pasan a una
**decisión PAPER independiente**, aprobada o rechazada, nunca a una autorización LIVE.

## Límites independientes de estrategias

Valores iniciales: 10.000 USDC, riesgo previsto por entrada 0,25%, pérdida diaria
1%, máximo dos posiciones, exposición bruta 20%, apalancamiento 1x. Se considera
correlacionada toda exposición del mismo sentido en los cuatro activos y se limita
al 15%. El riesgo agregado de stops no supera dos presupuestos individuales.
Las ganancias no incrementan la base de sizing por encima del capital inicial.
Cada símbolo admite una posición: no se promedia, no se aplica martingala.

El stop obligatorio queda a dos ATR(14), con suelo de distancia del 0,5% del precio;
el objetivo a dos veces esa distancia desde el peor precio previsto de entrada.
El sizing usa el peor nivel del lado ejecutable, redondeo adverso, comisión de
entrada/salida y slippage de salida. También respeta el presupuesto diario restante.
Se reduce la cantidad a `szDecimals` y se rechaza si no alcanza el mínimo de
10 USDC simultáneamente con los límites. La exposición no se compensa entre long
 y short. Un gap puede provocar pérdidas superiores al riesgo previsto del stop.

`PAPER_CONFIG_PATH` permite cargar un JSON completo al arrancar; ver
[config/paper.example.json](config/paper.example.json). Se pueden endurecer los
límites, ajustar costes y protecciones dentro de rangos validados. No se pueden
anular las reglas ni ampliar los máximos conservadores de esta entrega. Un archivo
con campos desconocidos o valores inválidos impide arrancar. La configuración se
conserva en la cuenta y debe coincidir en reinicios: cambiarla requiere una migración
explícita de cuenta, fuera de la API de esta fase. Las estrategias no reciben
acceso a esa configuración ni al estado mutable de la cuenta.

## Ejecución y posiciones

Solo órdenes IOC simuladas: recorren los hasta 20 niveles del lado opuesto del libro,
consumen como máximo el 10% de cada nivel y añaden 2 bps de slippage adverso, además
del spread existente. El precio se redondea con las reglas de perpetuos (cinco cifras
significativas, hasta `6-szDecimals` decimales; precios enteros válidos). Todas las
operaciones pagan 4,5 bps de taker, sin asumir descuentos de cuenta.

Cada lado de un snapshot solo admite un intento IOC por cuenta; el volumen no
usado se descarta conservadoramente. El resto de una orden parcial queda cancelado.
Los fills parciales pueden tener menos de 10 USDC: el mínimo se comprueba sobre la
orden solicitada. No se inventa liquidez para satisfacer el mínimo.

Las posiciones tienen stops y take profit persistentes. Se disparan con bid para
long y ask para short. Las salidas son reduce-only y nunca invierten la posición.
Una salida parcial queda pendiente y se reintenta con un nuevo libro, bloqueando
entradas. La falta de cotizaciones frescas impide simular el cierre: no se presupone
que un stop pueda ejecutarse al precio solicitado durante una desconexión.
El efectivo cambia por PnL realizado, comisiones y funding; equity añade PnL abierto
valorado por el lado de liquidación. Esa equity no descuenta hipotéticos costes de
cierre futuros. No se modelan liquidaciones del exchange ni colas de órdenes maker.

## Financiación horaria

Se consultan tasas históricas públicas mediante `fundingHistory`, con el mismo
pacing/reintentos del resto de HTTP. Una tasa positiva cobra a longs y paga a shorts.
La cantidad se toma en cada frontera horaria **antes** de procesar salidas. Cada
obligación tiene clave posición/hora y se liquida una sola vez, incluso si la posición
ya se cerró. El efectivo cambia en `-cantidad_firmada × oracle × tasa_horaria`.

El oráculo usado es la última observación de `activeAssetCtx` recibida en los 15 s
anteriores a la frontera. Es una aproximación explícita, no el precio exacto de
liquidación oficial: ese canal no incluye timestamp del exchange. Se persiste con
hora local y cada pago declara `oracle_model:last_observation_before_boundary`.
La tasa histórica se aplica tal como llega; no se vuelve a dividir entre ocho.

Si falta tasa u observación válida, la obligación permanece pendiente y bloquea
entradas; las salidas siguen permitidas con libros frescos. Un reinicio que cruce una
hora sin observación previa puede dejar esta incertidumbre sin resolución automática.
No se sustituye por cero ni por un precio posterior. Corregirla exige importar datos
históricos verificables; no hay endpoint para ignorarla. La recuperación procesa
hasta 48 horas por tick/petición. Una ausencia antigua de oráculo puede mantener el
bloqueo mientras requiere intervención y datos históricos. No se afirma equivalencia
exacta con el funding ni con fills de una cuenta real.

## Bloqueos, persistencia y recuperación

El trabajador revisa la cuenta aproximadamente cada segundo. Bloquea entradas por:

- Feed/análisis incompleto, libros u oráculos de más de 15 s, precio futuro, datos inválidos.
- Spread superior a 20 bps, profundidad de entrada inferior a 1.000 USDC o ATR/precio >3%.
- Cambio de precio >5% entre observaciones (enfriamiento de 60 s), o desvío >2% frente al oráculo.
- Límites de riesgo, posición existente, salida pendiente o financiación desconocida.
- Señales anteriores a activar la cuenta, futuras o de más de 120 s.
- Pérdida diaria: queda enclavada hasta el siguiente día UTC, cuando hay valoración y funding resueltos.

Una transacción bloquea la fila de cuenta, reconcilia efectivo/cantidades contra el
historial, procesa cierres y señales, guarda decisiones, órdenes/fills, funding y
snapshot. Todo se confirma o revierte junto. Los identificadores únicos impiden
consumo duplicado y dos trabajadores se serializan. No se usa un cursor de máximo
ID para señales: PostgreSQL puede confirmar IDs menores después de mayores.
Una discrepancia con el historial bloquea las transiciones; al arrancar impide
iniciar el servicio. Las variaciones de bloqueos se guardan en `paper_events`.

`/health` exige también un tick PAPER confirmado en los últimos 15 s. Un breaker
de riesgo normal se consulta en `account.blockers`; no significa que el proceso esté
averiado. Si falta valoración, `equity` conserva la última disponible y se señala
`valuation_unavailable`; no se debe presentarla como una valoración actual completa.
La reconciliación recorre el historial completo: adecuada para esta etapa, pendiente
de checkpoints verificados y retención para ejecuciones prolongadas.

Consultas de solo lectura: `/paper/account`, `/paper/positions`, `/paper/orders`,
`/paper/fills`, `/paper/decisions`, `/paper/funding`, `/paper/events`. Las cinco últimas
admiten `limit=1..500`; en fills limita las órdenes de origen, cada una puede contener
hasta 20 fills. No hay API de apertura manual, cambios de riesgo ni activación real.

## Fuentes oficiales contrastadas

- [Precisión de precios y tamaños](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/tick-and-lot-size)
- [Mínimo de órdenes y errores](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/error-responses)
- [Comisiones](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/fees)
- [Funding y precio oracle](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/funding)
- [Histórico de tasas](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/perpetuals)
- [Contextos WebSocket](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions)
