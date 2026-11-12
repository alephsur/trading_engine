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

- [x] Reevaluar SDK, firma, nonces, límites y reglas oficiales.
- [x] Implementar órdenes testnet, cancelaciones y reconciliación en CLI aislado.
- [x] Inyectar caídas, respuestas inciertas y fills parciales.
- [x] Validar órdenes firmadas, cancelación y cierre contra una cuenta testnet financiada.

Aceptación: órdenes de prueba trazables, sin duplicados, recuperación demostrada.
Aceptación externa completada el 11-11-2026: dos entradas BTC, fill parcial,
cancelación efectiva de cierre ALO, cierre IOC, replay sin duplicación y cuenta
final plana/sin órdenes. Evidencia en `docs/testnet-acceptance-2026-11-11.json`.
El planificador adapta tamaño/profundidad sin relajar los límites de entrada.

## Fase 8 — preparar LIVE sin activarlo

- [x] Frontera LIVE separada, sin carga de secretos y con configuración/confirmación independientes.
- [x] Runbook operativo, auditoría de reconciliación importada y pruebas de denegación por defecto.

Aceptación: LIVE permanece desactivado; activación requiere autorización explícita.
Preparación completada: `trading-live` audita snapshots offline; esta compilación
carece de firmante/transporte de producción y bloquea submit/cancel incondicionalmente.
No se presenta como integración mainnet ejecutable. Ver `LIVE.md`.

## Fase 9 — IA supervisada

- [x] Interfaces para contexto, explicación y análisis posterior.
- [x] Versionar propuestas y validarlas mediante backtesting y aprobación.
- [x] Tests que impidan introducir riesgo, ejecución o aprobación en respuestas de análisis.

Aceptación: IA solo asesora; toda modificación de estrategia pasa pruebas y aprobación.
Flujo offline completado con respuestas importadas, revisión interactiva y exportación
sin despliegue. Proveedor LLM no conectado; integración de un modelo concreto pendiente
de selección/configuración. Las revisiones son artefactos locales, no autenticación
multiusuario. Ver `ADVISOR.md`.

## Fase 10 — ciclo autónomo testnet previo a LIVE

- [x] 10A: observador BTC/EMA con velas testnet, sesiones acotadas, persistencia,
  pausa y recuperación del plazo tras reiniciar. Lectura externa validada.
- [x] 10B: conectar señal reciente con riesgo y ejecución; máximo una entrada
  por sesión, deduplicación durable y comprobación de apalancamiento 1x.
- [x] 10C (implementación): monitor continuo de pérdidas/protecciones/funding,
  perfil ruptura 1h y recuperación contrastada con exchange simulado y PostgreSQL.
- [x] Recuperación operativa: arranque único por cuenta, detección de pérdida
  de propiedad del supervisor y parada ordenada con Ctrl+C/SIGTERM.
- [ ] 10C (aceptación externa): señal natural de ruptura 1h, entrada automática,
  protección y salida reconciliadas en testnet. Mantener separada esta evidencia
  de las pruebas sintéticas y de la rentabilidad prospectiva PAPER.

El observador no ejecuta órdenes. LIVE continúa bloqueado. Ver `TESTNET.md`.

Estado 12-11-2026: código y comprobaciones offline consolidados. La recuperación
externa está bloqueada por PostgreSQL local no disponible; cuenta testnet sin
posiciones ni órdenes en consulta directa. Recuperar el historial original antes
de continuar 10C y medir la interrupción PAPER. Ver
`docs/phase10c-status-2026-11-12.json`.
