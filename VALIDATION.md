# Verificación — fase 7 (aceptación externa pendiente)

## Resultado local

- 60 pruebas Rust aprobadas: 52 sin PostgreSQL y 8 con PostgreSQL, incluidos schemas
  aislados para los dos nuevos escenarios de persistencia/reconciliación testnet.
- Firmas exactas de bracket y cancelación contrastadas con vectores independientes
  de `hyperliquid-python-sdk` 0.24.0, usando exclusivamente una clave dummy pública.
- Rechazo de transferencias, expiración inválida, activos no permitidos, precios o
  cantidades fuera de precisión, límites de tamaño/riesgo, posiciones preexistentes
  y datos obsoletos. Los cierres IOC/ALO son reduce-only.
- Concurrencia: una preparación por ID y nonces distintos; fallo anterior al POST
  produce cero envíos. Pérdida de respuesta o de persistencia del ACK deja estado
  incierto; ni reintentos concurrentes ni reinicio generan un segundo POST.
- Cuenta principal distinta del firmante, vinculación de API wallet, expiración
  sin envío y bloqueo persistente por pérdida diaria. Una intención incierta
  bloquea entradas nuevas aunque se cambie el identificador.
- Fills parciales/duplicados, discrepancias de cantidad y posición, persistencia
  idempotente y rechazo de replay conflictivo. `unknownOid` conserva incertidumbre.
- Consulta real de solo lectura a testnet: BTC, ETH, SOL y HYPE disponibles con
  índices descubiertos dinámicamente (3, 4, 0 y 135 en esta consulta del 6-11-2026).
- Rustfmt, Clippy y smoke de backtesting aprobados. El nuevo SDK y su lockfile no
  cambian los resultados deterministas previos. El CI incluye el smoke público
  testnet, sin claves ni escrituras firmadas.

**Pendiente:** verificar aceptación remota, ejecución, cancelación y cierre cuando
el mercado testnet permita superar los controles de entrada. Ningún fill de
las pruebas anteriores se presenta como una operación auténtica del exchange.
La fase 7 permanece abierta en `ROADMAP.md`; no se inicia fase 8.

GitHub Actions del commit `6b4786b`: aprobado, incluida construcción/arranque
Docker, dashboard contra el servicio real y recuperación tras reinicios.
[Ejecución 42220006328](https://github.com/alephsur/trading_engine/actions/runs/42220006328).

Revisión posterior `432a6a5`: los estados desconocidos, incluso si terminan en
`Canceled` o `Rejected`, conservan el bloqueo; `triggered` tampoco implica cierre.
Los estados reconocidos se interpretan mediante el enum del SDK fijado. Se excluye
también `.env.*` del contexto Docker. Formato, Clippy y las ocho pruebas del adaptador
(dos con PostgreSQL) aprobados localmente. Su
[CI 42221045513](https://github.com/alephsur/trading_engine/actions/runs/42221045513)
falló al consultar `/health` inmediatamente después de arrancar Docker:
`ConnectionResetError` escapaba de la espera de disponibilidad. Las pruebas Rust,
PostgreSQL, backtesting, lectura testnet, dashboard y construcción Docker habían
pasado; no llegaron a ejecutarse la recuperación ni el dashboard contra el servicio.

Corrección `4a90839`: la espera de arranque reintenta conexiones interrumpidas,
timeouts y respuestas HTTP 5xx dentro del presupuesto existente de 180 intentos.
HTTP 4xx y respuestas malformadas fallan inmediatamente. Solo se aplica a la
lectura de `/health`; no cambia el transporte de órdenes. Cuatro pruebas de
regresión Python aprobadas e incorporadas a CI cubren recuperación, agotamiento
del presupuesto, estado degradado persistente y respuestas inválidas.

[CI 42229938591](https://github.com/alephsur/trading_engine/actions/runs/42229938591)
aprobado, incluidos navegador, despliegue Docker y recuperación.

## Cuenta testnet configurada — 11 de noviembre de 2026

- `.env.testnet` disponible; PostgreSQL accesible y clave de API wallet cargada
  correctamente. El primer intento se bloqueó por permisos del archivo de clave;
  se corrigieron a `600` antes de continuar, sin mostrar su contenido.
- Cuenta sin posiciones ni órdenes abiertas, equity de prueba `499.758363` USDC.
- Propuestas de BTC/ETH rechazadas por profundidad ejecutable insuficiente;
  SOL por spread excesivo; HYPE por libro sin contraparte. Se comprobó también
  una propuesta BTC de aproximadamente 12 USDC, rechazada por profundidad.
  No se relajaron los controles ni se enviaron órdenes al exchange.
- Reconciliación final: cero solicitudes persistidas, cero posiciones, cero órdenes
  abiertas, `positions_reconciled=true` y equity sin cambios (`499.758363`).
- La carga local de la clave no demuestra autorización remota: sigue pendiente
  la aceptación de una orden firmada y el ciclo cancelación/cierre. Fase 7 abierta.

---

# Verificación — fase 6

## Resultado local

- 52 pruebas Rust aprobadas: 46 sin servicios externos y 6 con PostgreSQL.
- Pausa sin desactivar stops; controles persistentes, deduplicados y serializados
  con la ejecución; conflicto por reutilización de identificador y rechazo de
  acciones/campos no admitidos. Clave obligatoria y mismo origen en navegador.
- Reinicio mantiene la pausa; cierre sin libro queda pendiente; al volver un
  libro reciente se ejecuta una única salida. Snapshot, operaciones, costes y
  métricas insuficientes contrastados contra el ledger.
- Monitor persistente: drawdown intraminuto, huecos permanentes, exclusión del
  primer día parcial y rechazo de fronteras diarias ausentes.
- 4 pruebas Vitest: caducidad, valores desconocidos, carrera entre selecciones de
  mercado, reintento con el mismo identificador y huecos en gráficos.
- TypeScript y build de producción aprobados. Recursos de gráficos se cargan
  separadamente; Vite advierte del tamaño del módulo ECharts (559 kB sin comprimir,
  188 kB gzip), sin impedir la compilación.
- 2 escenarios Playwright aprobados: fixtures con confirmación de pausa, fallo de
  red y pantalla móvil de 390 px; recursos compilados servidos por Axum con datos
  reales de Hyperliquid, navegación Sistema, Prometheus y control no autorizado.
- Revisión visual de capturas de escritorio y móvil; sin desbordamiento horizontal.
- El servicio local alcanzó `/health` ready con los cuatro mercados y observaciones
  persistidas en PostgreSQL. No se habilitó ejecución real.
- Rustfmt, Clippy, formato frontend, smoke offline de backtesting y validación
  de Compose aprobados. Docker local no accesible: build/arranque verificados en CI.
- Smoke autenticado contra el proxy real de desarrollo: clave y origen rechazados
  cuando no corresponden, pausa/reanudación confirmadas, replay idéntico y conflicto
  de identificador. La clave temporal se generó en memoria y no se persistió.
- La inspección visual real detectó el sobre `{candle, closed}` de la API: corregido
  el desempaquetado; las fixtures usan ese contrato y excluyen velas provisionales.
  E2E comprueba píxeles dibujados en el canvas y ausencia de errores de consola.
- CI detectó un desbordamiento móvil no reproducido inicialmente en local. Se
  corrigieron navegación adaptable y contención de gráficos; tres repeticiones
  locales y el escenario remoto pasan. Servicios locales temporales detenidos.

[GitHub Actions: fase 6](https://github.com/alephsur/trading_engine/actions/runs/42215814891)
terminó con **success** sobre `a916d68`: Rustfmt, Clippy, 52 pruebas Rust,
PostgreSQL 17, smoke offline, formato/TypeScript/build frontend, 4 pruebas Vitest,
Playwright con fallos de red y pantalla móvil, Docker completo, datos públicos
reales, recuperación tras reinicio y dashboard servido por el contenedor con
verificación del gráfico y rechazo de controles sin autorización.
El resultado se registra en un commit posterior que solo modifica documentación.
Detalles de uso y límites en [DASHBOARD.md](DASHBOARD.md).

---

# Verificación — fase 5

## Resultado local

- 46 pruebas aprobadas: 41 sin servicios externos y 5 con PostgreSQL.
- Replay determinista, latencia de señales y coincidencia de fills con Account PAPER.
- Aislamiento temporal: modificar validación/prueba conserva resultados de entrenamiento
  y selección. Los tramos tienen cuentas independientes y no heredan posiciones.
- Rechazo de datos futuros, duplicados, orden incorrecto y fronteras inválidas;
  velas recibidas con retraso no producen ejecuciones antes de su disponibilidad.
- Métricas contrastadas con cálculos independientes: retorno, drawdown, win rate,
  profit factor y Sharpe; umbrales mínimos, varianza cero y métricas incompletas explícitas.
- Financiación publicada después de la frontera no se usa antes de recibirse;
  un oracle ausente conserva obligaciones y deja la rentabilidad final desconocida.
- PostgreSQL: exportación repetible de solo lectura, tiempos originales conservados,
  número de filas y cuentas sin cambios. Migración 0005 aplicada a una DB anterior.
- CLI completo con fixture sintético: dos ejecuciones producen informes idénticos;
  candidatos elegidos solo en entrenamiento; cambios en holdout no cambian la elección;
  un informe previo no puede sobrescribirse.
- Captura real previa: exportados 1.969 registros y ejecutado el CLI offline.
  La cobertura/muestra corta no cumple selección: `no_eligible_training_candidate`;
  no se fuerza un ganador ni se declara rentabilidad a partir de esa captura.
- Formato y Clippy sin advertencias. CI incluye el smoke offline y todas las
  integraciones PostgreSQL, además de las pruebas anteriores de Docker y recuperación.

[GitHub Actions: fase 5](https://github.com/alephsur/trading_engine/actions/runs/42211181556)
terminó con **success** sobre `23a8a94`: formato, Clippy, las 46 pruebas,
integraciones PostgreSQL 17, smoke offline de backtesting, construcción/arranque
Docker, datos reales y recuperación tras reinicio. El socket Docker local sigue
sin estar accesible; construcción/arranque se verificaron en CI. El resultado se
registra en un commit posterior que solo modifica documentación.

## Alcance

Se entregan replay de observaciones registradas y separación temporal, no evidencia
estadística de rentabilidad. No se simulan libros que no existen en la captura ni
cierres gratuitos al final del periodo. Las métricas de muestra insuficiente o costes
sin resolver son `null` con su estado. Modelos, límites de tamaño y comandos están
en [BACKTEST.md](BACKTEST.md). LIVE y TESTNET permanecen desactivados.

---

## Historial de verificación — fase 4

## Resultado local

- 37 pruebas aprobadas: 33 sin servicios externos y 4 con PostgreSQL.
- Riesgo decimal, mínimos/precisión, límites agregados y correlación, bloqueo de
  promediado, spread, profundidad, volatilidad, precios extremos y pérdida diaria.
- Fills de varios niveles, parciales IOC, cancelación de remanentes, cierres
  reduce-only parciales, protección contra reutilizar liquidez y stops con gaps.
- PnL long/short con comisiones de entrada/salida; funding de ambos signos, sin
  duplicados, obligaciones conservadas después del cierre y bloqueo si falta funding.
- Reinicio serializado determinista y rechazo de retrocesos del reloj.
- PostgreSQL: rollback de decisiones/fills/cuenta tras fallo inyectado; dos
  consumidores concurrentes producen una sola orden; reconexión preserva efectivo
  y posición; consumo de un ID confirmado tarde; historial inconsistente bloquea ejecución.
- API pública de tasas: rechaza activo incorrecto, tasa fuera de límites y horas duplicadas.
- Smoke con datos reales en servidor local y DB temporal separada: 16 series,
  análisis, cuenta PAPER, endpoints y trabajador activo. Una señal nueva de HYPE
  produjo una posición simulada con fills y comisiones. Las señales anteriores a
  activar la cuenta se rechazaron. Un reinicio conservó esa cuenta y su orden sin duplicarla.
- Formato, Clippy sin advertencias y configuración Compose válidos.

[GitHub Actions: fase 4](https://github.com/alephsur/trading_engine/actions/runs/42207830992)
terminó con **success** sobre `720052f`: formato, Clippy, las 37 pruebas, PostgreSQL 17,
construcción/arranque Docker, smoke con datos reales y reinicio con reparación de
histórico y conservación de la cuenta PAPER y su historial de órdenes/fills.
El socket Docker local sigue sin permitir acceso; esta comprobación se ejecutó en CI.
El resultado se registra en un commit posterior que solo modifica documentación.

## Alcance y límites

Las pruebas no establecen rentabilidad. El funding usa tasas históricas y un oracle
observado antes de la hora, no la liquidación exacta de una cuenta real. Una hora
sin oracle verificable conserva la obligación pendiente y bloquea entradas. En
[PAPER.md](PAPER.md) se describen los modelos, límites, recuperación e incertidumbres.
LIVE y TESTNET siguen rechazados; no se han enviado órdenes al exchange.

---

## Historial de verificación — fase 3

## Resultado local

- 22 tests aprobados: 19 sin servicios externos y 3 con PostgreSQL.
- Fixtures de EMA, RSI, ATR, MACD y volumen calculados mediante fracciones exactas
  y sumas ponderadas independientes; coincidencia dentro de 10^-20.
- Las cuatro estrategias cubren candidatos long/short y filtros; warmup y serie
  plana no generan señales; configuración inválida se rechaza.
- Pruebas de causalidad por prefijos, rechazo de velas futuras/no confirmadas,
  huecos, orden incorrecto, activos mezclados y replay determinista.
- Integración PostgreSQL: fallo inyectado tras escribir una primera señal revierte
  evaluaciones, eventos y cursor; dos consumidores concurrentes no duplican filas;
  un reinicio conserva resultados; distintas versiones mantienen históricos separados.
- Catch-up de más de 500 cierres usa lotes acotados y reproduce la misma evaluación.
- Smoke real de las 16 series: indicadores, cuatro decisiones por cierre,
  endpoints de señales/configuración y ejecución desactivada.
- Formato y Clippy sin advertencias. Migración aplicada sobre la DB de fases anteriores.

[GitHub Actions: fase 3](https://github.com/alephsur/trading_engine/actions/runs/42204097285)
terminó con **success** sobre `be11955`: formato, Clippy, tests, PostgreSQL 17,
build/arranque Docker, indicadores/evaluaciones reales de las 16 series y reinicio
con conservación de evaluaciones y ausencia de señales duplicadas.
Este resultado se registra en un commit posterior solo de documentación.

## Alcance

Es replay de decisiones, no backtesting de rentabilidad. Ninguna prueba establece
rentabilidad de una estrategia. No hay órdenes ni posiciones. Los límites de
histórico de mercado documentados en fase 2 siguen aplicándose. Las convenciones
de semilla, ventana y configuración están en [STRATEGIES.md](STRATEGIES.md).

---

## Historial de verificación — fase 2

## Resultado local

- 16 tests pasan: 14 unitarios y 2 de integración PostgreSQL. Se comprueban
  decimales, validación OHLC/libro, cierres, reintentos HTTP 429/5xx, respuestas
  de activo incorrecto, páginas acotadas, huecos internos, replay y persistencia.
- `cargo fmt --all -- --check` y Clippy con `-D warnings`: correctos.
- Migración 0002 aplicada sobre la DB ya existente de fase 1; tests de mercado
  usan además un esquema PostgreSQL aislado y lo eliminan al terminar.
- Smoke real: cuatro mercados con precios, libros y operaciones; 100 velas cerradas
  por cada una de las 16 series; consultas inválidas devuelven 400.
- Recuperación real: servicio detenido, eliminación de una vela histórica BTC/1m,
  reinicio y comparación del JSON recuperado con el original: idénticos.
- La recuperación mantiene el ancla persistida y el resultado final sin huecos.

Docker sigue sin permiso local de acceso al socket. El workflow ejecuta integración
PostgreSQL 17, build/arranque Compose, smoke real y una prueba de reinicio que elimina
y recupera una vela en la DB desechable de CI.

[GitHub Actions: fase 2](https://github.com/alephsur/trading_engine/actions/runs/42201266494)
terminó con **success** sobre `5a11d1a`: todos los checks, arranque, datos reales y
recuperación tras reinicio aprobados. Este resultado se registra en un commit
posterior que solo modifica documentación.

## Límites explícitos

La cobertura histórica de velas empieza en las 100 velas iniciales de cada serie.
Los huecos posteriores se buscan hasta ese ancla; los que salen de la ventana
recuperable del exchange permanecen sin resolver. El test unitario verifica el
límite de paginación/ventana, no una caída real de más de 5.000 intervalos.
Operaciones recientes pueden recuperar parte de una caída, pero no se garantiza
la cinta completa. Tampoco se reconstruyen libros intermedios perdidos. No hay
estrategias, órdenes ni posiciones en esta fase.

---

## Historial de verificación — fase 1

## Resultado local

- `cargo test --workspace --locked`: 6 tests unitarios pasan; 1 integración ignorada
  por defecto y ejecutada por separado.
- `cargo test -p trading-storage --locked -- --ignored`: integración pasa contra
  PostgreSQL 18.6 temporal. Verifica migraciones repetibles y evento JSON persistido.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: sin advertencias.
- `cargo fmt --all -- --check`: correcto.
- `docker compose config --quiet`: configuración válida.
- Smoke E2E real: metadatos de los cuatro perpetuos, precios positivos y decimales,
  API preparada, modo PAPER y ejecución desactivada.
- Consulta directa a PostgreSQL confirma eventos `startup`, `metadata` y `mids`.
- Se detuvo PostgreSQL durante la ejecución: `/health` devolvió HTTP 503 con
  `database:false`. Tras arrancarlo otra vez el smoke volvió a pasar.

La validación local de PostgreSQL usó binarios de paquetes oficiales de Ubuntu,
extraídos temporalmente sin instalar servicios ni modificar permisos del sistema.
Compose y CI usan PostgreSQL 17; la integración también pasó en esa versión en CI.

## Límite de verificación

El servicio Docker del equipo rechaza el acceso a `/var/run/docker.sock` y sudo
requiere autenticación interactiva. Se ha validado el archivo Compose, pero no
se ha ejecutado localmente el build de imagen ni `docker compose up`.
La limitación local quedó cubierta por [GitHub Actions](https://github.com/alephsur/trading_engine/actions/runs/42195631969),
con resultado **success** sobre el commit `38491e7`. Se verificaron formato,
Clippy, tests, integración PostgreSQL 17, construcción de imagen, arranque con
Compose y smoke E2E contra datos reales desde el contenedor. Después se detuvieron
los servicios de CI. Este informe se actualiza en un commit documental posterior.

No se ha simulado una desconexión de red externa ni un reinicio de órdenes:
no hay órdenes en esta fase. La frescura y el bloqueo de modos no admitidos
se verifican en tests unitarios. Velas, backfills, estrategias y riesgo quedan
fuera de esta entrega.
