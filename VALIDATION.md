# Verificación — fase 2

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
y recupera una vela en la DB desechable de CI. Resultado remoto pendiente del push.

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
