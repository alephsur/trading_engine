# Verificación — fase 4

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
