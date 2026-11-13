# Reintentos de lectura testnet — 13-11-2026

La sesión del 12-11 se pausó por HTTP 502 al consultar funding, sin consumir
su entrada. La causa quedó identificada por el diagnóstico persistente.
Se añadieron hasta tres intentos exclusivamente para lecturas transitorias de
`/info`, con esperas de 2 y 4 segundos y el limitador existente. Las órdenes
mantienen un solo intento de transporte. No se relajan frescura ni pausas.

Pruebas de recuperación tras dos 502, agotamiento, timeout/conexión/respuesta
incompleta, denegación de reintentos para escrituras, 429, 400 y JSON inválido
aprobadas. Tests de todos los targets Rust, 15 integraciones PostgreSQL,
clippy, formato y compilación conjunta aprobados. La comprobación externa previa
confirmó cuenta plana, sin bloqueos ni órdenes pendientes. PAPER sigue listo.

Se deshabilitó el arranque automático del servicio anterior, ya detenido;
su registro permanece pausado e intacto. El nuevo ensayo usa el servicio
`trading-testnet@breakout-btc-service-20261113.service`, 24 horas y una entrada
máxima de 20 USDC de notional planificado. La aceptación externa continúa
pendiente; arrancar el ensayo o superar pruebas sintéticas no la acredita.

# Diagnóstico y servicio testnet — 12-11-2026

Diagnósticos sanitizados con etapa, categoría y código HTTP; último fallo visible
incluso tras recuperación. Pruebas de contexto/redacción, persistencia, pausa y
evidencia de stop/objetivo aprobadas. Todos los targets Rust y las 15 integraciones
PostgreSQL pasan; tras la última ampliación de evidencia se repitieron los tests
testnet, incluidas sus nueve integraciones. Clippy, formato y build aprobados.

Plantilla systemd de usuario verificada e instalada. Servicio habilitado y activo
para `breakout-btc-service-20261112`, 24 horas y un intento; inicio 12-11-2026
21:20:13 UTC y vencimiento 13-11-2026 21:20:13 UTC. Linger habilitado para mantener
servicios de usuario al cerrar sesión. Una segunda instancia fue rechazada sin
pausar el servicio. No se ha inyectado una caída en esta nueva sesión externa;
los escenarios de recuperación con posición siguen siendo pruebas simuladas.

Primera supervisión externa correcta, cuenta plana/sin órdenes, sin bloqueos,
sin intento consumido. La vela observada no genera señal por filtro de volumen.
No se modificó la sesión anterior, pausada y vencida. PAPER sigue listo.
Evidencia: `docs/phase10c-service-2026-11-12.json`. La aceptación externa sigue
pendiente de señal natural, fill, protección y salida reconciliadas.

# Recuperación y corrección TLS — 12-11-2026

PostgreSQL recuperado. Las 15 pruebas de integración pasan. Formato, clippy,
tests de todos los targets y compilación conjunta del workspace pasan.
Se corrigió un panic del WebSocket al activar simultáneamente ring y aws-lc-rs:
el cliente ahora selecciona ring por conexión, con raíces públicas y validación
TLS estándar, sin depender del proveedor global. Incluye prueba de regresión.

El binario compilado conjuntamente arranca y pasa el smoke público completo:
precios, libros, operaciones, 16 series cerradas, indicadores y cuenta PAPER.
Salud lista en localhost:3001; el puerto 3000 pertenece a otro servicio.
Los parámetros de estrategia y riesgo coinciden con el manifiesto original.
Se recuperaron las velas; los libros y operaciones perdidos no se reconstruyen.
La interrupción de PAPER hasta el reinicio fue de 64.041 segundos, aproximadamente
17 h 47 min. El ensayo no constituye observación prospectiva continua de 30 días.

Supervisor testnet recuperado con sesión, plazo y presupuesto originales,
sin pausas ni bloqueos y sin entrada consumida al verificar. Continúa pendiente
la aceptación externa de una entrada y salida naturales. Evidencia:
`docs/phase10c-recovery-2026-11-12.json`. La validación de GitHub del commit
anterior `7ef3f92` finalizó correctamente.

# Verificación — consolidación fase 10C, 12-11-2026

Formato, clippy de todos los targets y tests Rust de todos los targets aprobados.
Dashboard: cuatro tests, compilación y formato aprobados. Smoke offline de
backtesting, cuatro tests de readiness y smoke del asesor aprobados.
Los ejemplos de investigación se han normalizado con rustfmt sin cambiar reglas;
las huellas históricas de sus archivos pueden diferir por formato.
Los datasets y resultados voluminosos permanecen locales en `artifacts/`;
el manifiesto congelado del seguimiento PAPER sí queda versionado.

No se han repetido las 15 pruebas de integración ignoradas: PostgreSQL configurado
en localhost:5433 no está accesible. Docker requiere permisos del usuario y sudo
requiere autenticación interactiva. No hay procesos locales del motor ni supervisor;
el puerto 3000 está ocupado por un servicio con otro contrato de salud.

Consulta directa de solo lectura a testnet: cuenta sin posiciones ni órdenes
abiertas. No se ha enviado ninguna orden ni creado otra sesión. Recuperar la
base original y consultar `breakout-btc-forward-20261111` antes de reanudar; mantener
su identificador, plazo y presupuesto. La aceptación externa sigue pendiente.
La continuidad de PAPER está interrumpida: cuantificar la laguna con la base
recuperada, sin presentar el periodo como 30 días de observación continua.
Evidencia: `docs/phase10c-status-2026-11-12.json`.

# Verificación — fase 9

- 69 pruebas Rust aprobadas en el workspace: 61 sin PostgreSQL y 8 con PostgreSQL.
- Cuatro nuevas pruebas de frontera: contexto futuro, huellas alteradas, parámetros
  inválidos, instrucciones tratadas como datos, campos riesgo/órdenes/aprobación
  rechazados y aprobación CLI denegada sin terminal interactiva.
- Smoke CLI completo con dataset sintético: backtesting real, riesgo del operador
  preservado, muestra mínima en holdouts, evidencia insuficiente bloqueada, rechazo
  de revisión ajena, exportación con recibo sintético y rechazo al cambiar experimento.
- La prueba cubre contexto que invade holdout y conservación de archivos existentes.
  El recibo de prueba no es una aprobación humana ni desplegó configuración alguna.
- Formato/Clippy aprobados. Ejemplos de prompt/ingesta ejecutados. No se llamó a
  ningún proveedor LLM ni se modificó una estrategia del servidor o cuenta remota.
- [CI fase 9](https://github.com/alephsur/trading_engine/actions/runs/43008950242)
  aprobado para `959ff86`, incluidos el nuevo smoke de asesoría, Docker, dashboard
  contra servicio real y recuperación. El proveedor LLM sigue sin configurar;
  existe el contrato Advisor y la implementación de respuestas importadas.

---

# Verificación — fase 8

- Cuatro pruebas del módulo LIVE aprobadas: combinaciones de los dos controles,
  bloqueo incondicional, discrepancias, stops, apalancamiento, duplicados y tiempo.
- Prueba del binario con variables LIVE activadas: submit/cancel fallan antes de
  abrir un archivo de orden o clave. No hay dependencias de red/firma en el módulo.
- Formato, Clippy y evaluación CLI de ejemplo aprobados. El informe distingue
  evidencia importada de una consulta autenticada al exchange.
- Ninguna clave ni orden de producción utilizada.
- [CI fase 8](https://github.com/alephsur/trading_engine/actions/runs/43007972158)
  aprobado para `19c5f29`, incluidos Docker, dashboard real y recuperación.

---

# Verificación — fase 7 (aceptación externa completada)

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

## Aceptación externa completada

El 11-11-2026 se ejecutaron dos ensayos acotados BTC contra testnet:

- Primer IOC: solicitado `0.00015`, ejecutado `0.00014` a `74694`; ambas protecciones
  aceptadas. El cierre ALO se ejecutó antes de cancelar: se conservó el rechazo de
  cancelación y se verificó el fill, sin confundirlo con una cancelación exitosa.
- Segundo IOC: `0.00026` a `75266`. Un cierre ALO quedó abierto, recibió cancelación
  exitosa y `orderStatus` confirmó `canceled`. Cierre final IOC: `0.00026` a `75191`.
- Las protecciones quedaron `reduceOnlyCanceled` al cerrarse las posiciones.
- Repetir el primer comando desde otro proceso conservó nonce/respuesta y número
  de solicitudes. No se duplicaron órdenes. Fallos de red/DB siguen probándose
  mediante inyección local, no mediante interrupción del exchange.
- Resultado final: cero posiciones, cero órdenes, siete solicitudes reconciliadas,
  equity inicial `499.758363`, final `499.757821` USDC de prueba. No se extrapola
  rentabilidad a partir de estos ensayos. La cuenta tenía margen aislado 2x; el
  harness no cambió esa configuración y limitó la exposición por nocional.
- Evidencia sin claves ni firmas en `docs/testnet-acceptance-2026-11-11.json`.

Cambio de código: el planificador adapta cantidad al 10% de profundidad y recorre
niveles cercanos conservando todas las validaciones de entrada. Siete pruebas de
modelo/firma y dos PostgreSQL aprobadas, además de formato y Clippy. La regresión
cubre ambos lados, reducción de tamaño, mínimo negociable, niveles demasiado
lejanos y datos obsoletos.

[CI 43005110185](https://github.com/alephsur/trading_engine/actions/runs/43005110185)
aprobado para `8fb9304`: formato, Clippy, todas las pruebas Rust/PostgreSQL,
backtesting, lectura pública testnet, dashboard/navegador, Docker y recuperación.

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

## Intentos previos a la aceptación — 11 de noviembre de 2026

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
  la aceptación de una orden firmada y el ciclo cancelación/cierre en ese momento;
  posteriormente completados según el registro de aceptación anterior.

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
