# AI Trading Engine

Fase 5 funcional en Rust: PostgreSQL, perpetuos BTC/ETH/SOL/HYPE, precios, libros,
operaciones y velas 1m/5m/15m/1h de Hyperliquid. Incluye recuperación histórica
y API de salud. Añade indicadores y cuatro estrategias sobre cierres confirmados.
Simula órdenes y posiciones con controles de riesgo, costes y recuperación persistente.
La ejecución real permanece desactivada. No necesita cuenta, wallet ni credenciales de Hyperliquid.

## Arranque con Docker

Requisitos: Docker con servicio accesible y Docker Compose.

```sh
docker compose up --build -d
curl http://127.0.0.1:3000/health
curl http://127.0.0.1:3000/markets
curl http://127.0.0.1:3000/prices
docker compose logs -f engine
```

El primer build tarda. Las migraciones se ejecutan automáticamente. `/health`
devuelve 503 hasta disponer de PostgreSQL, precios y libros recientes de los cuatro
mercados y 100 velas cerradas por serie (aproximadamente 40–60 segundos iniciales). `docker compose down` detiene servicios conservando el volumen.
Las credenciales incluidas son exclusivamente para desarrollo local.

## Desarrollo local

Requisitos: Rust (rustup instala la versión fijada), compilador C/linker, PostgreSQL
y acceso HTTPS/WSS a `api.hyperliquid.xyz`. Python 3 solo para el smoke test.

```sh
docker compose up -d postgres
cp .env.example .env
set -a
. ./.env
set +a
cargo run --locked -p trading-server
```

Con PostgreSQL existente, ajusta `DATABASE_URL` en `.env` y omite el arranque Docker.
El binario lee variables de entorno; no carga `.env` automáticamente.

| Variable | Valor por defecto | Uso |
| --- | --- | --- |
| `DATABASE_URL` | Obligatoria | Conexión PostgreSQL; nunca se registra explícitamente |
| `BIND_ADDR` | `127.0.0.1:3000` | Dirección de la API |
| `TRADING_MODE` | `PAPER` | Único valor admitido en esta fase |
| `STRATEGY_CONFIG_PATH` | Defaults integrados | Archivo JSON de parámetros, validado al arrancar |
| `PAPER_CONFIG_PATH` | Defaults integrados | Riesgo/costes PAPER; debe coincidir con la cuenta persistida |
| `RUST_LOG` | `info` | Nivel de logs JSON; `debug` muestra snapshots persistidos |

Los endpoints públicos de Hyperliquid están fijados en el adaptador; no se envían
peticiones autenticadas. Los índices de activos se descubren al conectar.

## API

- `GET /livez`: proceso vivo, independiente del estado de la DB/feed.
- `GET /health`: 200 listo o 503 degradado; incluye DB, frescura, recuperación histórica, análisis, trabajador PAPER y ejecución real desactivada.
- `GET /markets`: los cuatro mercados, precisión, leverage máximo e índice descubierto.
- `GET /prices`: precios como strings decimales, frescura y edad desde recepción local.

- `GET /candles?symbol=BTC&interval=1m&limit=100`: velas cerradas, orden descendente.
  Añade `closed_only=false` para incluir provisionales, cada una con su flag `closed`.
- `GET /trades?symbol=BTC&limit=100`: operaciones deduplicadas, con hora del exchange.
- `GET /book?symbol=BTC`: último snapshot (hasta 20 niveles/lado) y frescura.
- `GET /data-status`: cobertura de las 16 series y límites de completitud.
- `GET /strategies`: manifiesto de las cuatro estrategias, identidad de configuración y cursores.
- `GET /evaluations?symbol=BTC&interval=1m&limit=10`: indicadores, decisiones y motivos.
- `GET /signals?symbol=BTC&interval=1m&limit=100`: propuestas long/short, con `analysis_only:true`.

EMA, RSI, ATR, MACD y volumen se calculan con Decimal. Los parámetros de las
estrategias están en [config/strategies.example.json](config/strategies.example.json).
Para usar un archivo aprobado localmente, exporta `STRATEGY_CONFIG_PATH` con su ruta
antes de arrancar. En Docker debes montar el archivo y pasar esa variable al servicio.
No se cambian parámetros desde la API. Ver [reglas y convenciones](STRATEGIES.md).
La ausencia de señales es un resultado válido; ninguna estrategia promete rentabilidad.

Símbolos permitidos: BTC, ETH, SOL, HYPE. Intervalos: 1m, 5m, 15m, 1h. Límites: 1–500.
La recuperación conserva el ancla histórica tras reinicios y repara huecos internos.
Una vela solo queda cerrada después de confirmarse por HTTP; no se cierra por
inactividad del stream. Huecos fuera de la ventana de 5.000 velas del exchange
siguen visibles y bloquean la preparación: requieren datos históricos verificables,
no un reseteo silencioso. Operaciones antiguas y cambios intermedios del libro
pueden faltar tras una caída; la API no declara esos históricos completos.

Los precios antiguos pueden seguir visibles, siempre acompañados de `fresh:false`.
Son mids indicativos, no cotizaciones ejecutables. Los eventos se conservan en
`events` con tipos `startup`, `metadata`, `mids`, `capture_session` y `candle_recovery`.
Velas, operaciones y libros tienen tablas propias. Esta fase no aplica retención:
una ejecución prolongada aumentará el volumen de la base de datos.

La cuenta simulada y su auditoría están en `/paper/account`, `/paper/positions`,
`/paper/orders`, `/paper/fills`, `/paper/decisions`, `/paper/funding` y `/paper/events`.
Ver [modelo de ejecución, límites y financiación](PAPER.md).

## Backtesting offline

La fase 5 incorpora replay determinista, equity/drawdown, ratios con muestras mínimas
y selección de parámetros usando solo entrenamiento. Validación y prueba quedan
fuera de esa selección. Se exportan observaciones reales de PostgreSQL sin inventar
libros ni financiación. Consulta [BACKTEST.md](BACKTEST.md) para ejecutar el CLI,
configurar fechas y entender la cobertura y las métricas no disponibles.

## Verificación

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
TEST_DATABASE_URL="$DATABASE_URL" cargo test --workspace --locked -- --ignored
python3 scripts/backtest_smoke.py
python3 scripts/smoke.py http://127.0.0.1:3000
docker compose config --quiet
```

Las pruebas de integración requieren una base de pruebas: aplican migraciones y
verifican mercado, señales y cuenta PAPER en esquemas aislados. Se ejecutan explícitamente en CI.
El smoke E2E usa el servidor arrancado y datos reales; requiere Internet.
CI valida formato, lint, tests, integración PostgreSQL, construcción y arranque Docker,
las 16 series con datos reales, evaluaciones y recuperación tras reiniciar. `scripts/recovery_smoke.py`
elimina una vela de la base desechable de Compose para comprobar su recuperación;
**no lo ejecutes contra una base que quieras conservar intacta**.

Consulta [arquitectura](ARCHITECTURE.md), [roadmap](ROADMAP.md) y
[verificación de esta entrega](VALIDATION.md). Las siguientes fases añaden
dashboard, observabilidad e integración testnet; no están implementados en esta entrega.

## Dashboard PAPER

La fase 6 añade la interfaz en <http://localhost:3000/dashboard/> al arrancar
Compose. Incluye mercados, velas, cuenta, posiciones, actividad y estado del motor.
Los controles están deshabilitados por defecto; ver [DASHBOARD.md](DASHBOARD.md)
para configurar la clave local, conocer las métricas y ejecutar las pruebas.

## Integración testnet

El ejecutable independiente `trading-testnet` incorpora órdenes de prueba acotadas,
cancelaciones, cierres reduce-only y reconciliación persistente. El servidor y el
dashboard permanecen PAPER. Configuración, límites y procedimiento en
[TESTNET.md](TESTNET.md). La validación firmada de entrada, cancelación y cierre
contra testnet se completó; evidencia en
[registro de aceptación](docs/testnet-acceptance-2026-11-11.json). LIVE sigue desactivado.

## Preparación LIVE

`cargo run --locked -p trading-live -- status` confirma el bloqueo de producción.
La auditoría offline y el runbook están en [LIVE.md](LIVE.md). No carga claves ni
envía órdenes mainnet; PAPER y testnet conservan sus fronteras independientes.

## Análisis supervisado

`trading-advisor` registra respuestas de análisis importadas, ejecuta backtesting
y vincula la exportación de parámetros a una revisión manual. No modifica riesgo,
no despliega estrategias y no envía órdenes. No hay proveedor LLM conectado.
Flujo, ejemplos y límites de confianza en [ADVISOR.md](ADVISOR.md).
