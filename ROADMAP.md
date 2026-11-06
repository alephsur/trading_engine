# Roadmap

Se entrega una fase por iteración, con verificación, commit y push antes de pasar
a la siguiente. No se activa ejecución real sin autorización expresa.

## Fase 1 — base y lectura pública

- [x] Workspace modular, configuración por entorno y bloqueo de modos de ejecución.
- [x] PostgreSQL, migraciones automáticas y eventos persistentes.
- [x] Descubrimiento dinámico de BTC, ETH, SOL y HYPE.
- [x] Precios WebSocket, heartbeats, reconexión y detección de datos obsoletos.
- [x] API de salud, mercados, precios y logs estructurados.
- [x] Tests unitarios, integración PostgreSQL y smoke E2E real.
- [x] Dockerfile, Compose, CI, README y decisiones de arquitectura.
- [x] Verificar build y arranque de contenedores en GitHub Actions.

Aceptación funcional: compilación, fmt, clippy y tests pasan; DB conserva eventos;
`/health` llega a 200 con los cuatro mercados y devuelve 503 al caer PostgreSQL;
el servicio vuelve a estar listo tras recuperarse DB. Ver `VALIDATION.md`.

## Fase 2 — datos de mercado completos

- [x] Persistir velas 1m/5m/15m/1h con claves idempotentes y distinguir cierres.
- [x] Ingerir operaciones y order book con tiempo de exchange.
- [x] Recuperar huecos tras reconectar con límites y reintentos acotados.
- [x] Tests de duplicados, huecos, eventos fuera de orden y reinicio.

Aceptación: históricos y streams coherentes, sin duplicados; API separa velas provisionales de cierres confirmados. Operaciones y
libros conservan límites explícitos de cobertura; no se fabrican históricos perdidos.

## Fase 3 — indicadores y señales

- [x] EMA, RSI, ATR, MACD y volumen con fixtures calculados independientemente.
- [x] Interfaz Strategy y EMA trend, momentum/RSI, breakout y mean reversion.
- [x] Parámetros versionados, eventos de señales y replay determinista.

Aceptación: señales usan solo datos disponibles al cierre; pruebas sin look-ahead.

## Fase 4 — riesgo y paper trading

- [x] Reglas conservadoras, sizing decimal y restricciones oficiales del exchange.
- [x] PaperExecutor con spreads, fees, slippage, funding y liquidez.
- [x] Órdenes idempotentes, fills parciales, posiciones, stops y reconciliación.
- [x] Circuit breakers y auditoría de decisiones rechazadas.

Aceptación: ninguna entrada elude riesgo; incertidumbre bloquea nuevas entradas;
reinicios no duplican órdenes ni posiciones y PnL incluye costes.

## Fase 5 — backtesting y métricas

- [x] Reutilizar estrategias/riesgo con reloj y datos históricos deterministas.
- [x] Equity, drawdown, win rate, profit factor y Sharpe con muestra suficiente.
- [x] Separación temporal train/validation/test y evaluación fuera de muestra.

Aceptación: resultados reproducibles, sin leakage y con costes documentados.

## Fase 6 — dashboard y observabilidad

- [x] Vue 3, TypeScript, Vuetify y Pinia; seleccionar gráficos mantenidos.
- [x] Precios, velas, indicadores, señales, posiciones, historial y métricas.
- [x] Controles paper, alertas y telemetría; sin activación LIVE en frontend.

Aceptación cumplida: UI refleja estado persistido y degradación; controles y E2E
aprobados localmente y en CI, incluido Docker. Ver `DASHBOARD.md` y `VALIDATION.md`.

## Fase 7 — integración testnet

- [ ] Reevaluar SDK, firma, nonces, límites y reglas oficiales.
- [ ] Integrar órdenes testnet, cancelaciones y reconciliación.
- [ ] Inyectar caídas, respuestas inciertas y fills parciales.

Aceptación: órdenes de prueba trazables, sin duplicados, recuperación demostrada.

## Fase 8 — preparar LIVE sin activarlo

- [ ] Adaptador separado, secretos protegidos y doble bloqueo de activación.
- [ ] Runbook operativo, reconciliación y pruebas de denegación por defecto.

Aceptación: LIVE permanece desactivado; activación requiere autorización explícita.

## Fase 9 — IA supervisada

- [ ] Interfaces para contexto, explicación y análisis posterior.
- [ ] Versionar propuestas y validarlas mediante backtesting y aprobación.
- [ ] Tests que impidan acceso de IA a límites de riesgo y ejecución.

Aceptación: IA solo asesora; toda modificación de estrategia pasa pruebas y aprobación.
