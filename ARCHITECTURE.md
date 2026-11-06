# Arquitectura — fase 5

## Componentes ejecutables

Workspace Rust con siete crates:

- `trading-domain`: universo permitido y validación de metadatos, mids, OHLCV,
  operaciones y snapshots de libro. Precios/cantidades `rust_decimal`, strings JSON.
- `trading-market-data`: cliente HTTP público, pacing/reintentos y WebSocket con
  29 suscripciones: mids, cuatro libros, cuatro trades, 16 velas y cuatro contextos oracle.
- `trading-quant`: indicadores decimales, estrategias puras y replay determinista.
- `trading-paper`: cuenta, riesgo y ejecución simulada puros, con reloj/datos inyectados.
- `trading-backtest`: exportación de solo lectura, replay offline, métricas y separación temporal.
- `trading-storage`: PostgreSQL, migraciones embebidas, escrituras idempotentes y consultas.
- `trading-server`: Axum, supervisor, recuperación histórica concurrente y frescura.

El adaptador descubre de nuevo los índices de BTC, ETH, SOL y HYPE en cada conexión.
Ausencia, duplicados o mercados retirados impiden preparar el feed. No se cargan
claves, no se firman órdenes y no existe un cliente de ejecución real. PAPER simula con datos públicos; LIVE, TESTNET y BACKTEST se rechazan.

## Ingesta y consistencia

WebSocket y recuperación HTTP se ejecutan concurrentemente. El canal interno está
acotado a 256 mensajes. Las escrituras se confirman antes de publicar estado.
PostgreSQL impone un timeout de 5 segundos por sentencia. El supervisor revalida
metadatos tras errores, reconecta con backoff de 1–30 segundos y heartbeat de 20 s.

- `candles`: clave `(symbol, interval, open_time)`, contenido OHLCV decimal y flag
  `finalized`. Las actualizaciones WS son provisionales; un mensaje con menos o
  igual número de operaciones no sustituye uno más avanzado. HTTP confirma el
  cierre solo después del final del intervalo y 2 segundos de margen. Una vela
  finalizada es inmutable frente a replay WS/HTTP. No se inventan velas ausentes.
- `trades`: clave `(symbol, exchange_time, trade_id)`, porque `tid` solo no es
  globalmente único. Duplicados no crean filas nuevas. No se almacenan direcciones
  de compradores/vendedores: no son necesarias para este scanner.
- `books`: snapshots de hasta 20 niveles por lado, clave `(symbol, exchange_time)`.
  El libro actual se selecciona por mayor tiempo de exchange; un mensaje atrasado
  puede conservarse en el histórico sin reemplazar el estado más reciente.
- `candle_coverage`: ancla inicial persistente, extremo de la última comprobación
  y contador de huecos. El arranque siguiente no desplaza el ancla para ocultarlos.
- `events`: arranque, metadatos, mids, inicio de sesiones y resultado de recuperación.

Las validaciones comprueban activos, intervalos, límites temporales de velas,
OHLC consistente, cantidades no negativas y libros ordenados/no cruzados. Los
errores de parsing o almacenamiento invalidan la sesión. El cierre no se decide
por el mero paso del reloj sobre un último snapshot WS potencialmente incompleto.

## Recuperación

En el primer arranque se solicitan al menos 100 velas cerradas por activo/intervalo
o el tamaño de ventana de análisis configurado si es mayor. La ventana admite
60–500; ampliar la configuración puede extender el ancla hacia atrás, nunca
recortarla para ocultar huecos. En
reconexiones y cada 60 segundos se buscan TODOS los huecos desde el ancla, no
solo desde la vela más reciente. Las peticiones abarcan como máximo 500 velas.
Se respeta la ventana del exchange de 5.000 velas, con margen de dos; los huecos
más antiguos permanecen registrados y bloquean `history_ready`. La consulta
histórica permite ver datos parciales mientras el endpoint de salud siga degradado.

Un cliente HTTP compartido limita los comienzos de petición a uno cada 2 segundos.
Cada consulta pública admite hasta tres intentos ante fallos de transporte,
429 y 5xx; otros 4xx fallan inmediatamente. Los timeouts HTTP son 10 segundos.
Con páginas de hasta 500 velas, el ritmo queda por debajo del presupuesto oficial
por IP en operación propia; otras aplicaciones de la misma IP comparten el límite.

Al reconectar se consultan también `recentTrades` y se recibe un nuevo libro WS.
No hay garantía de reconstruir todas las operaciones perdidas ni los snapshots
intermedios del libro. La API publica `trade_history_complete:false` y
`book_history_complete:false`; cada nueva sesión deja un evento de discontinuidad.
No se debe usar ese histórico parcial como si fuera una cinta de mercado completa.

## Salud y API

`/livez` indica proceso vivo. `/health` exige DB accesible, mids recibidos hace menos
de 15 segundos, libros de los cuatro activos con hora de exchange reciente y una
comprobación histórica completa realizada hace menos de 180 segundos. Se toleran
hasta 5 segundos de diferencia futura del reloj para libros/operaciones; se requiere
un reloj del sistema sincronizado. En startup/reconexión se invalida el estado
histórico hasta completar la recuperación. Mids no incluye timestamp del exchange:
su frescura solo mide recepción local.

La fase 3 añade `signal_configs`, `signal_cursors`, `evaluations` y `signals`.
Las estrategias se ejecutan sobre cierres confirmados después de la recuperación.
Un lote de hasta 500 cierres por serie se guarda con su cursor en una sola
transacción; los bloqueos de cursor y claves únicas impiden duplicados. La tabla
`signals` sirve como flujo interno persistente con identificador secuencial.
Se guardan también las evaluaciones sin señal y sus motivos. Todas las propuestas
conservan su origen de análisis. El simulador evalúa por separado su elegibilidad PAPER. Ver [STRATEGIES.md](STRATEGIES.md).

`/health` exige además que las evaluaciones alcancen la cobertura actual.
`/strategies` expone parámetros/versiones; `/evaluations` incluye indicadores y
razones; `/signals` muestra propuestas direccionales de la configuración activa.

`/candles` devuelve únicamente velas finalizadas por defecto; `closed_only=false`
permite inspeccionar las provisionales. `/trades`, `/book` y `/data-status` exponen
los datos persistidos y sus límites. Límites de consulta: 1–500 filas, orden más
reciente primero. El servicio escucha en loopback por defecto y no tiene escrituras
expuestas por HTTP. Las tablas de alta frecuencia aún no tienen retención automática.

## SDK y evolución

Se mantiene el adaptador público pequeño. El SDK oficial revisado en fase 1 fue
`hyperliquid_rust_sdk` 0.6.0, con firma Alloy y WebSocket 0.20. Se reevaluará antes
de integrar órdenes testnet en fase 7; no se afirma compatibilidad de ejecución.

La fase 4 añade una cuenta PAPER transaccional y un trabajador separado de ingesta
que revisa cierres y propuestas cada segundo. El histórico de fills reconstruye
las posiciones y el efectivo; la comparación con el snapshot detecta discrepancias.
La financiación conserva obligaciones independientes de posiciones abiertas.
Ver [PAPER.md](PAPER.md) para estados, reglas, costes y limitaciones explícitas.

## Backtesting offline

El CLI independiente reutiliza Engine y Account con reloj inyectado. Los registros
se liberan por disponibilidad y las entradas esperan latencia explícita. Cada tramo
reinicia la cuenta y conserva solo warmup causal; el selector observa únicamente
entrenamiento. Los informes incluyen huellas, auditoría y estados de métricas.
La migración 0005 conserva la disponibilidad de tasas; `run` no accede a PostgreSQL
ni al exchange. Ver [BACKTEST.md](BACKTEST.md) para límites y convenciones.

## Referencias oficiales

- [Metadatos perpetuos](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/perpetuals)
- [Tipos WebSocket e identidad de operaciones](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions)
- [Candle snapshot y ventana histórica](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint#candle-snapshot)
- [Límites de uso](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/rate-limits-and-user-limits)
- [Heartbeats](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/timeouts-and-heartbeats)
- [SDK oficial: recentTrades](https://github.com/hyperliquid-dex/hyperliquid-rust-sdk/blob/master/src/info/info_client.rs)
