# Arquitectura — fase 1

## Alcance ejecutable

Workspace Rust con cuatro crates que tienen responsabilidades reales:

- `trading-domain`: universo permitido, metadatos y validación de precios con `rust_decimal`.
- `trading-market-data`: HTTP y WebSocket públicos de Hyperliquid, sin claves ni firma.
- `trading-storage`: PostgreSQL, migraciones SQLx embebidas y registro de eventos.
- `trading-server`: API Axum, supervisión de conexión, frescura y apagado ordenado.

Flujo actual: metadatos → validación del universo → persistencia → suscripción
`allMids` → validación de los cuatro precios → persistencia → publicación en API.
Los índices se calculan a partir de la posición en `universe` en cada conexión.
Un mercado ausente, duplicado, inválido o retirado impide declarar el feed listo.
Los mids son precios indicativos; no son fills ni precios garantizados de ejecución.

## Decisiones

Se usa un adaptador HTTP/WebSocket pequeño en vez del SDK de ejecución. Se revisó
el SDK oficial `hyperliquid_rust_sdk` 0.6.0: incorpora Alloy y firma de órdenes,
reqwest 0.12 y tokio-tungstenite 0.20. La última actualización devuelta por GitHub
en esta revisión es del 2025-10-21. No se afirma compatibilidad de ejecución:
se reevaluará con pruebas de testnet en fase 7. El protocolo público cubre esta
entrega sin introducir credenciales ni dependencias de firma.

Los importes usan Decimal y se serializan como strings, sin conversiones a float.
Los eventos llevan una secuencia PostgreSQL y hora de recepción del servidor.
`allMids` no aporta aquí una hora de exchange: la frescura mide recepción local,
no latencia verificable del exchange. Cada snapshot completo se confirma en DB
antes de publicarse. El canal interno tiene capacidad limitada (32 mensajes).
No hay promesa de recuperar ticks perdidos: el siguiente snapshot restablece
precios actuales; velas y recuperación histórica pertenecen a fase 2.

El supervisor revalida metadatos al reconectar, aplica backoff 1–30 segundos y
heartbeats cada 20 segundos. Los timeouts son 10 segundos para HTTP/conexión,
5 para escrituras y 15 para ausencia de precios completos. La salud comprueba DB
y datos recientes. Fallos de persistencia invalidan la sesión; no se mantienen
colas ilimitadas. El reintento vuelve a 1 segundo tras una sesión estable de 60 s.

`/livez` indica proceso vivo; `/health` devuelve 503 cuando no está preparado.
La API expone solo lecturas y escucha en loopback por defecto. Compose también
publica únicamente en loopback. Los logs son JSON. PostgreSQL es obligatorio al
arrancar: un fallo inicial termina el proceso. Tras el arranque el supervisor
recupera las sesiones fallidas automáticamente.

Solo se acepta `TRADING_MODE=PAPER`. En esta fase PAPER significa lectura pública:
aún no simula órdenes. No existe cliente de ejecución, endpoint de órdenes ni
carga de claves. LIVE, TESTNET y BACKTEST se rechazan explícitamente.

## Evolución prevista

Los seis componentes lógicos (scanner, validación de riesgo, sizing, executor,
logger y monitor de equity) se introducirán cuando tengan comportamiento y tests.
El pipeline futuro será MarketData → Signal → Validation → RiskAssessment →
Sizing → Order → Fill → PositionMonitoring → Exit → Reporting. Sus transiciones
necesitarán identidades, idempotencia, transacciones y recuperación persistente.
La tabla actual es un diario de ingesta, no un motor de event sourcing completo.

Riesgo será independiente de estrategias y de IA. Fase 4 aplicará los valores
iniciales: 10.000 USDC, 0,25% por operación, 1% pérdida diaria, dos posiciones,
20% nocional, 1x y stop obligatorio. Ningún ajuste desde estrategias o LLM podrá
anular límites. El adaptador LIVE será separado y seguirá desactivado sin
configuración y autorización explícitas.

## Referencias oficiales consultadas

- [Metadatos perpetuos](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/perpetuals)
- [Suscripciones WebSocket](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions)
- [Heartbeats](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/timeouts-and-heartbeats)
- [SDK Rust oficial](https://github.com/hyperliquid-dex/hyperliquid-rust-sdk)

Precisión de órdenes, mínimos negociables, límites de uso, nonces y reglas de
firma se verificarán nuevamente antes de implementar sizing y ejecución.
