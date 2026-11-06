# Verificación — fase 1

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
Compose y CI usan PostgreSQL 17; la ejecución de esa versión queda pendiente de CI.

## Límite de verificación

El servicio Docker del equipo rechaza el acceso a `/var/run/docker.sock` y sudo
requiere autenticación interactiva. Se ha validado el archivo Compose, pero no
se ha ejecutado localmente el build de imagen ni `docker compose up`.
CI incluye integración con PostgreSQL 17 y build de imagen. Su resultado debe
comprobarse tras el push; no se considera aprobado solo por crear el workflow.

No se ha simulado una desconexión de red externa ni un reinicio de órdenes:
no hay órdenes en esta fase. La frescura y el bloqueo de modos no admitidos
se verifican en tests unitarios. Velas, backfills, estrategias y riesgo quedan
fuera de esta entrega.
