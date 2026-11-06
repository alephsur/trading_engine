# AI Trading Engine

Primera fase funcional en Rust: PostgreSQL, metadatos de perpetuos BTC/ETH/SOL/HYPE,
precios públicos de Hyperliquid por WebSocket y API de salud. No ejecuta ni simula
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
devuelve 503 hasta disponer de PostgreSQL y precios recientes de los cuatro
mercados. `docker compose down` detiene servicios conservando el volumen.
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
- `GET /health`: 200 listo o 503 degradado; incluye DB, frescura y ejecución desactivada.
- `GET /markets`: los cuatro mercados, precisión, leverage máximo e índice descubierto.
- `GET /prices`: precios como strings decimales, frescura y edad desde recepción local.

Los precios antiguos pueden seguir visibles, siempre acompañados de `fresh:false`.
Son mids indicativos, no cotizaciones ejecutables. Los eventos se conservan en
`events` con tipos `startup`, `metadata` y `mids`. Esta fase no aplica retención:
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
CI valida formato, lint, tests, integración PostgreSQL y construcción Docker.

Consulta [arquitectura](ARCHITECTURE.md), [roadmap](ROADMAP.md) y
[verificación de esta entrega](VALIDATION.md). Las siguientes fases añaden velas,
estrategias, riesgo, simulación y dashboard; no están implementadas en esta entrega.
