# Integración testnet — fase 7

`trading-testnet` es un ejecutable independiente para validar órdenes contra
Hyperliquid testnet. El servidor, el dashboard y los trabajadores de fases 1–6
siguen siendo PAPER. No conectan señales automáticamente a este adaptador.
El único destino de escritura es `https://api.hyperliquid-testnet.xyz/exchange`;
no hay un parámetro para cambiarlo ni se siguen redirecciones.

## Estado de aceptación

Implementación y pruebas deterministas disponibles. La consulta pública real
confirmó BTC, ETH, SOL y HYPE en testnet el 6 de noviembre de 2026, con índices
descubiertos de nuevo (distintos de mainnet). **La aceptación de órdenes firmadas
contra el exchange requiere una cuenta testnet financiada y una API wallet aprobada.**
El 11 de noviembre se verificó la configuración local y una cuenta financiada;
las propuestas se rechazaron antes de enviar por falta de profundidad, spread
excesivo o libro vacío. La cuenta terminó plana y sin solicitudes enviadas.
La autorización remota del firmante todavía no está demostrada.
Mientras esa prueba no esté registrada, la fase no se considera validada de extremo
a extremo. Las fixtures no se presentan como ejecuciones reales.

## Preparación local

1. Preparar una cuenta **exclusiva de testnet**, sin posiciones ni órdenes ajenas,
   y aprobar una API wallet dedicada para este proceso. No usar la clave de la
   cuenta principal ni reutilizar una API wallet revocada. No se admiten vaults,
   subcuentas, spot, HIP-3, transferencias ni retiros en este ejecutable.
2. Guardar la clave de la API wallet en un archivo privado **fuera del repositorio**.
   El archivo contiene únicamente la clave hexadecimal. En Linux debe ser accesible
   solo por su propietario (`chmod 600`). Nunca introducir la clave como argumento
   de terminal, en el navegador, en un commit ni en la conversación.
3. Copiar `.env.testnet.example` a `.env.testnet` (ignorado por Git). Configurar
   `TESTNET_ACCOUNT` con la dirección **principal**, `TESTNET_PRIVATE_KEY_FILE` con
   la ruta absoluta del archivo y `DATABASE_URL` con PostgreSQL. El programa carga
   este archivo, sin sobrescribir variables del entorno ya definidas.
4. `TESTNET_CONFIRM=TESTNET_ONLY` habilita únicamente las escrituras testnet de
   este ejecutable. El servidor PAPER no interpreta esta opción. Se requiere
   saldo de pruebas suficiente (mínimo 100 USDC de testnet, además de pasar sizing).

```sh
docker compose up -d postgres
cargo run --locked -p trading-testnet -- markets
cargo run --locked -p trading-testnet -- reconcile
```

El usuario proporciona la cuenta y su aprobación de API wallet; el código no crea
cuentas, no reclama faucets, no transfiere fondos y no solicita credenciales mainnet.

## Probar una operación acotada

Crear un plan sin firmar ni enviar, con precios actuales y sizing conservador:

```sh
cargo run --locked -p trading-testnet -- plan BTC buy > /tmp/testnet-entry.json
cargo run --locked -p trading-testnet -- check probe-entry-0001 /tmp/testnet-entry.json
cargo run --locked -p trading-testnet -- submit probe-entry-0001 /tmp/testnet-entry.json
cargo run --locked -p trading-testnet -- reconcile
cargo run --locked -p trading-testnet -- watch
```

`plan` también admite `sell`, ETH, SOL y HYPE. Si cambian los precios, `check` o
`submit` pueden rechazar el plan: generar otro **solo si el anterior nunca fue
aceptado para envío**. Nunca cambiar de identificador para sortear una solicitud
incierta. El mismo identificador y la misma intención no generan una segunda firma
ni un segundo intento de transporte. Una intención distinta con el mismo ID falla.

El archivo de entrada es JSON estricto, por ejemplo:

```json
{"kind":"entry","symbol":"BTC","buy":true,"quantity":"0.0002","limit":"75000","stop":"74250","take_profit":"76500"}
```

Es un ejemplo de estructura, **no precios vigentes**. `plan` evita calcular precios
y cantidades manualmente. Los límites se vuelven a comprobar al enviar:

- Cuenta plana y sin órdenes abiertas; entradas anteriores reconciliadas.
- Nocional de 10–25 USDC de testnet y hasta el 15% del patrimonio; el plan apunta
  como máximo a 20. No se aumenta la exposición de una posición existente.
- Riesgo estimado hasta el límite del stop, incluida tolerancia de ejecución y
  costes conservadores, ≤0,25% del patrimonio. No es garantía de precio de salida.
- Spread ≤20 bps; límite de entrada a ≤20 bps del medio; participación ≤10% de la
  profundidad ejecutable. Libro válido y menor de 15 s; preflight menor de 15 s.
- Precisión decimal de precio/cantidad según metadatos; sin conversión financiera
  a coma flotante. Stop y objetivo obligatorios en el lado correcto.
- Una entrada IOC con stop/objetivo reduce-only, agrupación `normalTpsl`. No se
  asume que todas las patas hayan sido aceptadas: se reconcilian individualmente.
- Hasta diez intentos de entrada por día UTC y bloqueo persistente tras perder 1%
  respecto al primer patrimonio observado al preparar una entrada ese día.

Este es un banco de integración controlado, **no el despliegue autónomo de las
estrategias PAPER**: sus stops de prueba no sustituyen el sizing ATR, filtros de
estrategia ni supervisión de producción. No hay ajuste de apalancamiento o margen;
el límite de nocional acota el apalancamiento efectivo del ensayo.

## Cierre, cancelación y limpieza

Para cerrar la posición con una orden IOC reduce-only:

```json
{"kind":"close","symbol":"BTC"}
```

Guardar como archivo y ejecutar `submit` con un ID nuevo, seguido de `reconcile`.
El cierre admite un fill parcial; consultar la posición y usar otro ID **solo tras
reconciliar el anterior** si queda cantidad. No cambia de dirección ni reabre una
posición aunque el snapshot previo haya envejecido.

Para probar una orden que repose y su cancelación, un cierre puede incluir un
`limit` decimal válido a menos del 1% del precio actual. Esa variante usa ALO y
sigue siendo reduce-only. Puede ser rechazada si cruza el mercado o ejecutarse
antes de cancelarla. La cancelación se dirige por el ID local del cierre:

```json
{"kind":"cancel","target":"probe-close-resting-0001"}
```

Cancelar nunca elimina stops ni objetivos. Para retirar protecciones restantes
tras cerrar completamente, usar:

```json
{"kind":"cleanup","target":"probe-entry-0001"}
```

La limpieza exige cuenta plana, padre confirmado terminal, ninguna orden abierta
que no sea protección reduce-only y 30 s desde la expiración de todas las firmas
de entrada. Se cancelan únicamente las patas protectoras de la solicitud elegida.
Un error por orden ya cancelada se conserva y se verifica por consulta; no se
interpreta como éxito de una orden distinta.

## Persistencia, nonces y resultados inciertos

Migración 0007: nonces por firmante, solicitudes, fills únicos, snapshots, eventos
y límites diarios, todos en tablas `testnet_*`. No se escriben tablas de cuenta PAPER.

Preparación bloquea cuenta y firmante en PostgreSQL, asigna `max(reloj, nonce+1)`,
firma y persiste. El firmante queda vinculado a una sola cuenta. `cloid` se deriva
de red/cuenta/ID/pata; tiene 128 bits. Las firmas siempre usan el dominio testnet,
sin vault y con `expiresAfter` de diez segundos. Nonce, acción y firma no cambian
al reintentar el comando.

Antes del POST se confirma la transición `prepared → uncertain`. Solo un proceso
puede reclamar una solicitud. Se hace **un intento de transporte**; no hay retry
automático de escrituras. Un timeout, error HTTP, cuerpo inválido, caída de proceso
o fallo de PostgreSQL posterior deja incertidumbre persistente. Si el proceso cae
entre la reclamación y el envío, puede que nunca se haya enviado: se prefiere
bloquear a duplicar. Una solicitud preparada que caduca sin reclamar pasa a `expired`.

Los estados de transporte (`acknowledged`, `rejected`, `uncertain`) y el resultado
de reconciliación son independientes. Un HTTP 200 no equivale a fill. Un rechazo
de todas las patas se distingue de aceptación mixta o respuesta desconocida.
Una solicitud incierta **no se vuelve a firmar ni enviar**: `reconcile` consulta
cada `cloid`, fills y estado de la cuenta principal. `unknownOid` sigue siendo
desconocido, incluso tras expirar; no permite nuevas entradas automáticamente.

Los fills se deduplican por cuenta/oid/tid. Un replay diferente del mismo fill
falla. Se contrastan cantidad original, remanente y fills decimales; el neto de
fills se compara con la posición del exchange. Las consultas no son una transacción
remota: una divergencia transitoria conserva el bloqueo hasta la próxima lectura.
La ausencia de datos no genera fills ficticios. Stops rechazados o desconocidos
requieren atención; este ejecutable no promete una salida automática exitosa.

`watch` solo consulta y persiste, aproximadamente cada diez segundos después de
terminar el ciclo; no genera órdenes. Una sesión compartida limita cada petición
a dos segundos, con timeout de diez segundos y cuerpos de hasta 8 MB. Procesos
externos comparten los límites por IP y cuenta: usar una API wallet dedicada.

## Límites operativos explícitos

- Máximo 100 solicitudes por cuenta en este banco; el exceso bloquea reconciliación
  y exige exportación/revisión, no purga silenciosa. Hasta 2.000 fills por respuesta;
  alcanzar el límite bloquea. El exchange solo conserva los 10.000 fills recientes:
  no se afirma cobertura histórica completa. Una cantidad no conciliada bloquea.
- No se usa WebSocket privado: reconciliación REST determinista, sin prometer latencia
  de producción. Las protecciones aceptadas residen en el exchange.
- No se vinculan estrategias del scanner mainnet al ejecutor testnet. El dashboard
  sigue mostrando únicamente PAPER; las cuentas nunca se mezclan.
- El archivo de clave se mantiene en memoria protegida durante su carga. PostgreSQL
  guarda firmas de corta vigencia para trazabilidad, pero no la clave. El informe
  CLI omite las firmas; no se publica un endpoint HTTP de control testnet.
- El registro diario observa equity al preparar entradas; no es un monitor continuo
  de drawdown testnet. El CLI de ensayos no sustituye el monitor PAPER.

## SDK, firma y pruebas

Se revisaron `hyperliquid_rust_sdk` 0.6.0 y `hypersdk` 0.3.0. Se usa **hypersdk
0.3.0**, listado actualmente en la documentación de Hyperliquid, con firma tipada,
expiración y normalización decimal. Sus tipos generan el orden correcto de campos
MessagePack. Sus dependencias Alloy/HyperEVM amplían el lockfile, pero no se enlazan
al servidor PAPER. El SDK tiene licencia MPL-2.0; se consume como dependencia sin
modificar su código. El transporte, persistencia y límites son propios.

Vectores independientes generados offline con `hyperliquid-python-sdk==0.24.0`,
clave dummy pública y cero llamadas de red, verifican la firma exacta de un bracket
y una cancelación. Regenerador: `scripts/testnet_signing_fixture.py`. El fixture se
versiona; CI no instala Python SDK ni necesita una clave.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
TEST_DATABASE_URL=postgres://… cargo test --workspace --locked -- --ignored
python3 scripts/testnet_public_smoke.py
```

Las pruebas PostgreSQL usan schemas aislados y transportes de prueba explícitos:
concurrencia, firma única, nonces monotónicos, caída antes del POST, respuesta
perdida, fallo al guardar ACK, reinicio, expiración, bloqueo diario, fills parciales,
duplicados y divergencias de posición. No representan fills auténticos de testnet.

## Fuentes revisadas

- [API y SDK recomendados](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api).
- [Exchange, cloid y expiresAfter](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint).
- [Firma](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/signing).
- [Nonces y API wallets](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/nonces-and-api-wallets).
- [Precisión](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/tick-and-lot-size).
- [Estados, fills y cobertura](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint).
- [Límites](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/rate-limits-and-user-limits).
- [hypersdk](https://github.com/infinitefield/hypersdk) y
  [firma Python oficial](https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/master/hyperliquid/utils/signing.py).
