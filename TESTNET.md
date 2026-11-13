# Integración testnet — fase 7

`trading-testnet` es un ejecutable independiente para validar órdenes contra
Hyperliquid testnet. El servidor, el dashboard y los trabajadores de fases 1–6
siguen siendo PAPER. No conectan señales automáticamente a este adaptador.
El único destino de escritura es `https://api.hyperliquid-testnet.xyz/exchange`;
no hay un parámetro para cambiarlo ni se siguen redirecciones.

## Estado de aceptación

Aceptación externa completada el 11 de noviembre de 2026 con una API wallet
autorizada y fondos de prueba. Se verificaron entrada parcialmente ejecutada,
protecciones reduce-only, cierre ALO, cancelación efectiva, cierre IOC y replay de
una solicitud tras reiniciar el CLI sin duplicación. La cuenta terminó sin posiciones
ni órdenes abiertas, con todos los fills reconciliados. Evidencia real separada
de las fixtures: [registro de aceptación](docs/testnet-acceptance-2026-11-11.json).
Las pruebas de fallos de transporte/DB siguen siendo inyecciones locales; no se
afirma haber provocado fallos del exchange. El ensayo no valida producción LIVE.

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

`plan` también admite `sell`, ETH, SOL y HYPE. Recorre niveles del libro desde el
mejor precio y limita la cantidad al 10% de la profundidad ejecutable. Devuelve la
primera propuesta que supera todos los controles, incluida la distancia máxima
de 20 bps al medio y el mínimo de 10 USDC; si ninguna cumple, rechaza el plan.
Si cambian los precios, `check` o
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

## Observador automático BTC/EMA — fase 10A

El observador consulta exclusivamente velas de **testnet**, evalúa EMA 20/50
sobre 100 velas cerradas de un minuto y persiste la decisión. No carga la clave,
no firma, no envía órdenes y no consume señales del servidor PAPER.

```sh
cargo run --locked -p trading-testnet -- observe-once btc-ema-001 config/testnet-observer.json
cargo run --locked -p trading-testnet -- observe-run btc-ema-001 config/testnet-observer.json
cargo run --locked -p trading-testnet -- observe-status btc-ema-001
cargo run --locked -p trading-testnet -- observe-pause btc-ema-001
```

La configuración incluida dura 15 minutos y consulta cada 30 segundos. El plazo
empieza con la primera consulta, también con `observe-once`. Reutilizar el ID
conserva plazo, configuración y pausa incluso tras reiniciar; para otra sesión
se necesita otro ID. La pausa es permanente para ese ID. Ctrl+C detiene el proceso,
pero no persiste una pausa. No se instala ningún servicio en segundo plano.

Se rechazan huecos, duplicados, mercados incorrectos, ventanas antiguas y cambios
en una evaluación ya guardada para la misma vela. Las velas en formación quedan
fuera del cálculo y hay dos segundos de margen tras el cierre. Un error detiene
el proceso; `observe-status` muestra la última evaluación correcta, cuya fecha
debe consultarse para no confundirla con una lectura actual. La expiración impide
guardar nuevas observaciones, incluso si ocurre durante una petición.

`max_entries: 1` registra la preferencia acordada para el futuro ejecutor;
**este observador permite cero entradas**. No es una aprobación de ejecución
ni un monitor de posiciones, pérdidas, financiación o protecciones. El ejecutor separado de la fase 10B, descrito a continuación, exige una
activación adicional. No se requieren credenciales de mainnet.

Validación real del 11-11-2026: lectura y persistencia de 100 velas cerradas,
EMA `hold/no_ema_cross`, sin firma ni envío. Las pruebas con velas sintéticas
se mantienen separadas de esta comprobación externa.

## Ejecutor BTC/EMA de un intento — fase 10B

Disponible únicamente en el CLI testnet, separado del observador y del servidor
PAPER. Requiere ambas confirmaciones: `TESTNET_CONFIRM=TESTNET_ONLY` de la
configuración existente y `TESTNET_AUTO_CONFIRM=BTC_EMA_ONE_ENTRY`. Ejemplo:

```sh
TESTNET_AUTO_CONFIRM=BTC_EMA_ONE_ENTRY cargo run --locked -p trading-testnet -- auto-run btc-ema-auto-001 config/testnet-observer.json
cargo run --locked -p trading-testnet -- auto-status btc-ema-auto-001
cargo run --locked -p trading-testnet -- auto-pause btc-ema-auto-001
```

`auto-once ID config.json` realiza un ciclo con la misma autorización. El ID
conserva la fecha inicial, el plazo, la pausa y el presupuesto al reiniciar.
Solo se consideran cruces EMA de velas cerradas **después de activar la sesión**
y con antigüedad máxima de 120 segundos. No se fuerza una entrada si no hay cruce.

Antes de preparar una entrada: reconciliación de cuenta plana/sin órdenes,
planificación con libro reciente, límites existentes de liquidez y riesgo,
preflight actualizado y consulta `activeAssetData` que exige BTC aislado a 1x.
El tamaño máximo es 20 USDC, riesgo máximo 0,25% del equity, participación máxima
10% de profundidad ejecutable y stop basado en dos ATR (mínimo aproximado 0,5%
del precio), con objetivo a dos veces esa distancia. El guard existente exige
distancia al stop <=3%, riesgo dentro de límite incluyendo colchón de ejecución
y costes, mínimo nocional 10 USDC, y límites diarios de entrada/pérdida. Si el
tamaño resultante no cumple, se rechaza sin relajar ninguna restricción.

El presupuesto es **un intento**, no un fill garantizado: se consume en PostgreSQL
antes de firmar. Un rechazo, caída o error posterior no lo devuelve. Una señal
consumida tampoco puede repetirse usando otro ID. La pérdida de oportunidad al
caer entre reserva y firma es intencionada. La pausa/expiración/vigencia se
comprueban de nuevo al reclamar el envío; una orden ya reclamada y en vuelo puede
llegar después de pedir pausa. Las respuestas inciertas nunca se reenvían.

El proceso termina al consumir el intento o vencer el plazo; los errores intentan
persistir una pausa y detienen el proceso. Ctrl+C o SIGTERM quedan registrados
durante el ciclo y persisten una pausa cuando este termina.
Un fallo de base de datos puede impedir guardarla, pero no habilita un reenvío de
un intento consumido. El informe distingue reserva de intento y estado real de
transporte; no interpreta una reserva como una ejecución.

**Límite operativo:** envía un bracket IOC con stop/objetivo reduce-only y hace
reconciliación posterior. No es todavía un servicio continuo de protección,
financiación y pérdidas para el perfil histórico BTC/EMA. El perfil supervisado
de ruptura 1h se describe a continuación. El fin de sesión o la pausa no cierran
una posición ni cancelan protecciones. Revisar cuenta/órdenes y usar `reconcile`
o `watch` para seguimiento; el ensayo debe ser supervisado. Una ACK no garantiza
que todas las patas estén aceptadas: comprobar el informe y la cuenta.

Ensayo supervisado del 11-11-2026: sesión de 120 segundos completada por
expiración, sin cruce EMA ni intento de entrada. Se verificó BTC aislado a 1x.
Esto valida lectura/reconciliación/espera/expiración, no un fill automático.
Evidencia: `docs/testnet-auto-acceptance-2026-11-11.json`.

## Supervisión y ruptura confirmada 1h — fase 10C

`config/testnet-confirmed-breakout.json` selecciona exactamente la estrategia
`confirmed_breakout_1h_v1` de PAPER, sobre 100 velas confirmadas **de testnet**.
Admite BTC, ETH, SOL o HYPE como activo de una sesión. Mantiene **un intento por
sesión**, cuenta plana antes de entrar, importe planificado máximo 20 USDC de
prueba, margen aislado 1x, sizing ATR y todos los límites existentes. Los datos
mainnet de PAPER no disparan órdenes testnet. La configuración histórica permanece
compatible y conserva sus sesiones/identificadores.

### Monitor sin claves ni órdenes

```sh
cargo run --locked -p trading-testnet -- supervise-once
cargo run --locked -p trading-testnet -- supervise-run
cargo run --locked -p trading-testnet -- supervise-status
```

El monitor coteja posiciones con fills, órdenes abiertas con los identificadores
reconciliados y stop/objetivo con la intención firmada. Exige lado opuesto,
cantidad suficiente, tipo correcto, precio de disparo y `reduceOnly`. Una ACK o
`waitingForTrigger` no acreditan protección. Lee y conserva funding sin sumarlo
de nuevo al patrimonio que proporciona el exchange.

La pérdida del 1 % diario se observa en cada ciclo, aunque no haya entradas.
La referencia diaria es la primera observación persistida de patrimonio; las
transferencias externas también afectan esa variación, por lo que la cuenta debe
seguir siendo exclusiva del ensayo. El bloqueo se mantiene hasta el siguiente
día UTC, incluso si el saldo se recupera o se reinicia el proceso. Una sesión
pausada permanece pausada al cambiar de día.

Al activar supervisión se crea un control de cuenta: todas las entradas, incluidas
las manuales, necesitan un ciclo correcto de menos de 60 segundos. Se vuelve a
comprobar al reclamar el envío, además de la frescura de libros y preflight de
15 segundos. Errores de lectura/persistencia, divergencias o protecciones faltantes
bloquean entradas y pausan las sesiones. La recuperación permite nuevas sesiones
solo después de un ciclo correcto; nunca reanuda automáticamente una sesión pausada.

`supervise-run` consulta de nuevo diez segundos después de terminar cada ciclo.
La duración de la reconciliación se añade a ese plazo: no es una garantía de
reacción de diez segundos. Sigue vigente el límite de 100 solicitudes del banco
testnet. Una respuesta de 500 registros de funding bloquea, sin afirmar cobertura
completa ni avanzar el cursor. Los cambios de estado quedan en `testnet_events` y
la situación actual en `supervise-status`; no se envían avisos externos.

### Ensayo automático supervisado

```sh
TESTNET_AUTO_CONFIRM=CONFIRMED_BREAKOUT_ONE_ENTRY \
  cargo run --locked -p trading-testnet -- auto-run ruptura-btc-001 config/testnet-confirmed-breakout.json
```

También exige `TESTNET_CONFIRM=TESTNET_ONLY`. El ejemplo admite entradas durante
24 horas como máximo; cada sesión conserva su plazo, presupuesto y configuración
tras reiniciar. La deduplicación incluye activo, intervalo, estrategia y cierre.
La cuenta sigue limitada a una posición de prueba a la vez.

La vigilancia continúa después de consumir el intento, pausar o caducar el plazo
de entrada. El proceso termina normalmente cuando ya no puede entrar y confirma
cuenta plana, sin órdenes y con solicitudes resueltas. Si falta un stop, se supera
la exposición permitida, el margen no es aislado 1x o se activa el límite diario,
el ejecutor puede realizar **un solo intento de cierre IOC reduce-only** de la
posición atribuida a su entrada. No toca posiciones de origen incierto.

Ese cierre conserva un identificador por entrada. Respuesta perdida, caída tras
prepararlo, rechazo o ejecución parcial no autorizan otro envío: la vigilancia
continúa y requiere revisión del operador si queda cantidad. No retira los stops
aceptados ni sustituye una posición no reconciliada por una estimación.
El monitor sin firmante solo informa/bloquea; los cierres requieren `auto-run`.

`auto-once` se rechaza para este perfil porque dejaría de supervisar tras una
entrada. Ctrl+C o SIGTERM quedan registrados incluso durante una consulta.
El proceso termina el ciclo en curso, persiste el bloqueo/pausa y sale antes de
iniciar otro ciclo. No se cancela una petición en vuelo: ese ciclo aún puede
completar una entrada o un cierre. La parada no cierra por sí misma posiciones
ni elimina protecciones. Reanudar el mismo `auto-run` recupera la supervisión,
manteniendo la pausa y el presupuesto consumido.

### Arranque único y recuperación operativa

`auto-run`, `auto-once`, `supervise-run` y `supervise-once` adquieren un bloqueo
de supervisor por cuenta en PostgreSQL antes de trabajar. Un segundo supervisor
sale con error sin cargar el firmante ni pausar al primero. Para consultar uno
en marcha, usar `auto-status` o `supervise-status`, que no requieren ese bloqueo.
Los observadores de velas y las consultas de estado pueden seguir funcionando.

El bloqueo ocupa una conexión dedicada que se cierra al salir; no se devuelve
bloqueada al pool. Si se pierde esa conexión, el proceso falla en la siguiente
comprobación de ciclo e intenta persistir el bloqueo de entradas. No recupera
la conexión en silencio. Este mecanismo evita supervisores concurrentes mientras
la conexión permanece viva; la protección contra envíos duplicados sigue siendo
la reclamación durable de cada orden, incluso ante una partición de red.

Tras una caída abrupta, arrancar **el mismo ID y el mismo archivo de configuración**.
Se recuperan la fecha de inicio, el vencimiento original, las reservas de intento,
las solicitudes inciertas y las pausas. Nunca cambiar el ID para recuperar un
intento o ampliar el plazo. El primer ciclo vuelve a reconciliar con el exchange
antes de evaluar entradas. Una parada ordenada mantiene la pausa al reiniciar.
SIGKILL, apagado o pérdida de DB pueden impedir persistirla; en ese caso el control
de supervisión caduca a los 60 segundos y la reserva durable sigue evitando reenvíos.
El servicio de usuario descrito más abajo permite recuperar el mismo proceso
tras fallos. No protege frente a suspensión, apagado o indisponibilidad de PostgreSQL.

Reinicio externo verificado el 11/11/2026 con cuenta plana: la sesión conservó
su configuración, plazo e intento disponible, y completó una nueva reconciliación.
Un segundo proceso fue rechazado antes de cargar la clave sin pausar al primero.
Los casos con posición abierta o transporte incierto se mantienen identificados
como pruebas con exchange simulado. Evidencia en
[registro de recuperación](docs/testnet-recovery-acceptance-2026-11-11.json).

### Alcance de validación

Las pruebas con PostgreSQL y exchange simulado cubren: fill parcial sin stop,
cierre con respuesta perdida y reinicio sin duplicación, protecciones verificadas,
sesión pausada/caducada que sigue monitorizando, bloqueo por monitor obsoleto y por
pérdida diaria persistente, fallo de red y conflictos en registros de funding.
La equivalencia de análisis 1h se contrasta para los cuatro activos.

Estas pruebas no acreditan un fill automático externo de la nueva estrategia.
Su aceptación externa requiere una señal natural nueva y completar entrada,
protección, salida y reconciliación. Testnet tampoco demuestra la rentabilidad
de la estrategia: esa evaluación continúa en PAPER sobre datos mainnet nuevos.

Estado verificado el 11/11/2026: 71 pruebas Rust y 14 con PostgreSQL aprobadas;
supervisión externa correcta y ensayo corto expirado sin entradas. Se inició un
ensayo BTC de hasta 24 horas y un intento, pendiente de señal natural. Evidencia
y límites en [registro de supervisión](docs/testnet-supervision-acceptance-2026-11-11.json).

Contratos revisados: [funding y estado de cuenta](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/perpetuals),
[órdenes abiertas](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint)
y [comportamiento de TP/SL en fills parciales](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/take-profit-and-stop-loss-orders-tp-sl).

### Recuperación del 12-11-2026

Tras arrancar PostgreSQL se recuperó `breakout-btc-forward-20261111`, sin crear
una sesión nueva, ampliar su plazo ni restablecer su presupuesto. El supervisor
volvió a reconciliar sin bloqueos; no había posiciones ni órdenes abiertas.
El plazo original para entradas termina el 12-11-2026 a las 16:20:38 UTC
(17:20:38 en Madrid). No se prolonga automáticamente si no aparece señal.
El proceso sigue sujeto a la disponibilidad del ordenador y de PostgreSQL.
Evidencia: `docs/phase10c-recovery-2026-11-12.json`.

### Diagnóstico persistente y servicio de usuario

Los errores del supervisor conservan etapa, categoría y, cuando existe, código
HTTP. Se distinguen timeout, conexión, transporte, respuesta incompleta, JSON
inválido, base de datos y determinadas validaciones. Nunca se registran cuerpos
de respuesta, claves ni mensajes arbitrarios. `supervise-status` y `auto-status`
incluyen `last_failure`, incluso después de un ciclo correcto. Mientras hay una
posición, `protection_observed` conserva la evaluación y las órdenes de protección
observadas; también registra la transición a cuenta plana. Esto permite auditar
la protección después del cierre, sin confundir ACK con protección verificada.
La pausa de entradas
sigue siendo permanente para la sesión; no se relaja por recuperar conectividad.

`python3 scripts/install_testnet_service.py` instala y valida la plantilla systemd
de usuario, sin iniciar sesiones. Requiere compilar previamente el binario y
conservar este checkout y `.env.testnet`. Para un ID aprobado y un ensayo nuevo:

```sh
systemctl --user enable --now trading-testnet@ID.service
systemctl --user status trading-testnet@ID.service
journalctl --user -u trading-testnet@ID.service --no-pager -n 100
```

El servicio usa el perfil BTC de 24 horas y un intento máximo de 20 USDC de
notional planificado. Reinicia tras una salida anómala con el mismo ID, presupuesto
y vencimiento; hasta cinco inicios en cinco minutos, con 30 segundos entre intentos.
Una sesión pausada o vencida termina normalmente al confirmar cuenta plana,
sin órdenes y sin solicitudes pendientes, y no se reinicia automáticamente.
Nunca cambia el ID ni desbloquea entradas para recuperar un intento.

`systemctl --user stop trading-testnet@ID.service` solicita parada ordenada,
con hasta 120 segundos para finalizar el ciclo; persiste pausa pero no cierra por
sí mismo posiciones ni retira protecciones. Una caída forzada puede impedir esa
persistencia: la reserva de órdenes y el control de frescura siguen vigentes.
No detenerlo con posición abierta sin un plan de supervisión alternativo.

Con `loginctl enable-linger "$USER"` los servicios de usuario pueden mantenerse
al cerrar sesión y arrancar al iniciar el equipo. PostgreSQL y la red deben estar
disponibles; el servicio no inicia Docker ni evita la suspensión. El proceso PAPER
actual conserva su arranque independiente. Los avisos quedan en el journal y en
la base, sin notificaciones externas configuradas.

### Lecturas ante fallos transitorios — 13-11-2026

Una consulta de solo lectura a `/info` admite hasta tres intentos en total ante
HTTP 502/503/504, timeout, fallo de conexión o respuesta incompleta. Las pausas
entre intentos son de 2 y 4 segundos y cada petición conserva el limitador de
frecuencia existente. Los reintentos se registran con diagnóstico sanitizado en
el journal. No se reintentan 429, otros errores HTTP, JSON inválido ni validaciones.

Las órdenes de `/exchange` conservan un único intento de transporte, incluso
ante timeout o HTTP 502. Agotar los intentos de lectura propaga el error y mantiene
la pausa durable existente. La frescura de snapshots y el bloqueo de entradas
siguen vigentes; un reintento no vuelve válido un snapshot obsoleto. Nunca se
reanuda una sesión pausada ni se reinicia su presupuesto automáticamente.

El ensayo `breakout-btc-service-20261112` quedó pausado tras un HTTP 502 al leer
funding, sin entrada, y se conserva intacto. Esta corrección evita que un único
fallo transitorio de lectura termine inmediatamente un ensayo futuro.
