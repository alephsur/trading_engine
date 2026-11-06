# AI Trading Engine

Fase 2 funcional en Rust: PostgreSQL, perpetuos BTC/ETH/SOL/HYPE, precios, libros,
operaciones y velas 1m/5m/15m/1h de Hyperliquid. Incluye recuperación histórica
y API de salud. No ejecuta ni simula
órdenes todavía. No necesita cuenta, wallet ni credenciales de Hyperliquid.

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
| `RUST_LOG` | `info` | Nivel de logs JSON; `debug` muestra snapshots persistidos |

Los endpoints públicos de Hyperliquid están fijados en el adaptador; no se envían
peticiones autenticadas. Los índices de activos se descubren al conectar.

## API

- `GET /livez`: proceso vivo, independiente del estado de la DB/feed.
- `GET /health`: 200 listo o 503 degradado; incluye DB, frescura, recuperación histórica y ejecución desactivada.
- `GET /markets`: los cuatro mercados, precisión, leverage máximo e índice descubierto.
- `GET /prices`: precios como strings decimales, frescura y edad desde recepción local.

- `GET /candles?symbol=BTC&interval=1m&limit=100`: velas cerradas, orden descendente.
  Añade `closed_only=false` para incluir provisionales, cada una con su flag `closed`.
- `GET /trades?symbol=BTC&limit=100`: operaciones deduplicadas, con hora del exchange.
- `GET /book?symbol=BTC`: último snapshot (hasta 20 niveles/lado) y frescura.
- `GET /data-status`: cobertura de las 16 series y límites de completitud.

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

## Verificación

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
TEST_DATABASE_URL="$DATABASE_URL" cargo test -p trading-storage --locked -- --ignored
python3 scripts/smoke.py http://127.0.0.1:3000
docker compose config --quiet
```

La prueba de integración requiere una base de pruebas: aplica migraciones y escribe
un evento. Está ignorada en la suite sin DB y se ejecuta explícitamente en CI.
El smoke E2E usa el servidor arrancado y datos reales; requiere Internet.
CI valida formato, lint, tests, integración PostgreSQL, construcción y arranque Docker,
las 16 series con datos reales y recuperación tras reiniciar. `scripts/recovery_smoke.py`
elimina una vela de la base desechable de Compose para comprobar su recuperación;
**no lo ejecutes contra una base que quieras conservar intacta**.

Consulta [arquitectura](ARCHITECTURE.md), [roadmap](ROADMAP.md) y
[verificación de esta entrega](VALIDATION.md). Las siguientes fases añaden
estrategias, riesgo, simulación y dashboard; no están implementadas en esta entrega.
