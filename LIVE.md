# Preparación LIVE — fase 8

Esta entrega prepara la frontera de producción y su auditoría offline. **No es un
ejecutor mainnet habilitable**: no contiene firmante, carga de claves ni transporte
de órdenes. `trading-live submit` y `cancel` fallan antes de leer archivos o entorno.
No hay endpoint, feature de Cargo ni variable que retire este bloqueo. El servidor
y dashboard conservan PAPER; no dependen de este crate.

## Comprobaciones disponibles

```sh
cargo run --locked -p trading-live -- status
cargo run --locked -p trading-live -- assess config/live-assessment.example.json
```

`assess` contrasta posiciones esperadas/importadas, IDs de órdenes, duplicados,
universo permitido, máximo dos posiciones, apalancamiento hasta 1x, frescura menor
de 15 segundos y protección stop reduce-only de cantidad suficiente y lado opuesto.
La muestra usa una fecha deliberadamente antigua: no representa datos actuales.
Los snapshots deben proceder de una captura independiente verificada del exchange;
el programa no los descarga ni acredita su autenticidad. No evalúa equity, riesgo
agregado o calidad de ejecución LIVE: esas verificaciones son requisitos futuros,
no capacidades implícitas de este informe.

La salida siempre incluye `execution_enabled=false` y `production_build_disabled`.
Las comprobaciones `activation.enabled` y `manual_confirmation` son independientes;
la frase exacta de confirmación se define en `MANUAL_CONFIRMATION`. Incluso con
ambas satisfechas no se obtiene autorización de ejecución. Un informe consistente
solo acredita coherencia de los datos importados.

## Secretos y separación

No proporcionar claves de producción a esta versión. El formato de evaluación
rechaza campos desconocidos, incluidos secretos; los resultados no incluyen el
contenido de la confirmación. `.env.*` está excluido de Git y del contexto Docker.
Las claves testnet permanecen fuera del repositorio y nunca se reutilizan en LIVE.
Una futura versión autorizada requerirá proveedor de secretos dedicado, API wallet
nueva y exclusiva, permisos `600` o gestor de secretos, rotación y errores sin claves.

## Runbook previo a una futura activación

1. Mantener entradas bloqueadas y registrar release, configuración de riesgo y
   evidencia testnet. La aceptación de testnet no autoriza mainnet.
2. Obtener autorización explícita para una versión de producción concreta.
   Implementar/revisar por separado firma mainnet y transporte; no cambiar el
   endpoint testnet. No retirar el bloqueo de este release.
3. Verificar cuenta/API wallet mediante consultas a la cuenta principal; descubrir
   índices, precisión, mínimos y costes vigentes. Confirmar margen/apalancamiento
   1x, stops y límites agregados con el exchange.
4. Recuperar ledger, fills, nonces y solicitudes pendientes de almacenamiento
   separado de PAPER/testnet. Contrastar posiciones/órdenes del exchange. Ante
   ausencia de datos o discrepancias, bloquear entradas y conservar evidencia.
5. Evaluar riesgo con el motor conservador; persistir intención, nonce y firma
   antes del envío y reclamación antes del POST. Nunca repetir escrituras inciertas
   ni cambiar su ID para evitar el bloqueo.
6. Ante timeout, caída o respuesta ambigua: consultar cloid/oid, fills y cuenta.
   Una cancelación puede perder la carrera frente a un fill. HTTP 200 no implica fill.
7. Ante pérdidas o desconexión: bloquear entradas y conservar stops aceptados.
   Cerrar solo con datos fiables y órdenes reduce-only. Si no se puede verificar
   el cierre, escalar al operador, sin bucle de órdenes automáticas.
8. Para rollback: detener entradas, reconciliar, confirmar posiciones/protecciones,
   conservar DB/auditoría y revocar la API wallet cuando proceda. Un reinicio no
   debe borrar nonces ni reactivar solicitudes.

Los pasos 2–8 son condiciones para trabajo futuro autorizado. Este release no
ejecuta esos procedimientos contra mainnet ni solicita fondos de producción.
