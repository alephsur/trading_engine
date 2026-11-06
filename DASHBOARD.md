# Dashboard y observabilidad (fase 6)

El dashboard se sirve en `/dashboard/` desde el mismo proceso que la API. Usa
Vue 3, TypeScript, Vuetify, Pinia y Apache ECharts. Todas las cifras provienen de
PostgreSQL o del feed público; las pruebas de navegador usan fixtures aisladas.

## Arranque

`docker compose up --build` construye también la interfaz. Abrir
<http://localhost:3000/dashboard/>. No hace falta un servicio frontend adicional.

Desarrollo local (Node 24):

```sh
npm ci --prefix web
npm run build --prefix web
DATABASE_URL=postgres://trading:local_only@localhost:5433/trading cargo run -p trading-server
```

Para desarrollo con recarga, `API_URL=http://127.0.0.1:3000 npm run dev --prefix web`.
Vite publica `/dashboard/` en el puerto 5173 y redirige `/api` al backend. En el
servidor, `DASHBOARD_DIR` permite cambiar el directorio de recursos compilados.

## Controles PAPER

Por defecto son de solo lectura. Para habilitar controles, configurar
`PAPER_CONTROL_TOKEN` con una clave aleatoria de al menos 32 caracteres ASCII
imprimibles sin espacios, en el entorno del servidor o `.env` de Compose (ignorado
por Git). Introducir esa misma clave en «Desbloquear controles». No se guarda en
localStorage, cookies, URL, eventos ni logs. Recargar la página la borra.
La API y los controles están pensados para el entorno local; si se exponen fuera
del equipo, el operador debe proporcionar HTTPS y control de acceso al despliegue.

- **Pausar:** impide entradas; conserva stops, objetivos, funding y salidas.
- **Iniciar / reanudar:** vuelve a evaluar entradas sin saltarse ningún límite.
- **Detener y cerrar:** pausa entradas y marca todas las posiciones para salida.
- **Cerrar:** marca la posición elegida para salida y pausa nuevas entradas.

«Detener» no apaga el proceso: debe seguir recibiendo datos para proteger y cerrar
las posiciones. Solicitar un cierre no garantiza un fill inmediato. Sin libros
recientes o con poca liquidez, se mantiene pendiente y puede ejecutarse en partes.
La configuración de riesgo y estrategias es de solo lectura. LIVE sigue rechazado.

`POST /paper/control` exige `Authorization: Bearer …`, el mismo origen cuando hay
cabecera Origin y JSON estricto, por ejemplo:

```json
{"command_id":"identificador-unico-0001","action":"pause"}
```

Acciones: `pause`, `resume`, `close_all`, `close_position` (esta última exige
`position_id`). Los identificadores admiten 8–64 caracteres alfanuméricos o guiones.
Se guarda la petición y su respuesta en la misma transacción que el cambio de
cuenta y el evento de auditoría. El bloqueo de cuenta es el mismo que usa el worker.
Un reintento idéntico devuelve la respuesta original; reutilizar el identificador
para otra acción devuelve 409. Ante un resultado incierto, el navegador conserva
la solicitud y reutiliza su identificador. No muestra confirmación optimista.

## Vistas y calidad de datos

Resumen: patrimonio, rentabilidad desde el capital inicial, PnL abierto, mercados,
velas/EMA, RSI/ATR, posiciones y stops, curva de patrimonio y resultados por activo.
Actividad permite inspeccionar órdenes (incluidos fills), señales evaluadas por
PAPER, operaciones, funding y eventos. Sistema expone salud, contadores, alertas,
parámetros de estrategias y cobertura. Todos los horarios son UTC.

La interfaz consulta cada cinco segundos, no solapa ciclos y expira la cuenta tras
15 segundos sin actualización. Las consultas tienen timeout de siete segundos.
Una selección de mercado descarta respuestas de selecciones anteriores. Los datos
obsoletos se señalan y las valoraciones desconocidas se muestran como «—».

`GET /observability` entrega una instantánea coherente de PostgreSQL con aislamiento
repeatable-read. Los precios y salud del proceso se consultan por separado. Conserva
50 órdenes/decisiones/eventos/funding, 100 operaciones y los últimos 1440 minutos
observados de curva en la respuesta. Las métricas por operación usan todo el ledger.
La respuesta no incluye secretos. Las rutas originales siguen disponibles y tienen
alias bajo `/api/` para el navegador.

## Métricas y límites

La migración 0006 añade comandos, observaciones de patrimonio por tick y un monitor
persistente. La curva muestra la última observación de cada minuto; los huecos no
se rellenan. El drawdown se calcula con cada tick (aproximadamente un segundo),
antes de reducir la curva. Su ventana comienza al instalar esta fase, **no** al
crear una cuenta anterior. Es un drawdown observado, no un máximo intratick.

Una valoración/funding desconocidos o una interrupción superior a 15 segundos
marcan permanentemente esa ventana como incompleta; reiniciar no borra esa marca.
Se sigue mostrando el drawdown observado con su limitación. El Sharpe queda nulo
si la ventana es incompleta. Se necesitan 30 retornos diarios UTC, positivos en sus
bases, con varianza distinta de cero; se excluye el primer día parcial. Reutiliza
la definición de fase 5 (tipo libre de riesgo cero; anualización IID explícita en
API). Aciertos y profit factor requieren 30 operaciones cerradas y conciliadas;
no se convierten muestras insuficientes en ceros.

Las observaciones y el ledger no se purgan automáticamente. La consulta de métricas
recorre el historial completo: para cuentas de larga duración habrá que introducir
agregados incrementales y una política explícita de retención antes de escalar.

## Telemetría y pruebas

`GET /metrics` publica formato Prometheus: uptime, desconexiones, errores PAPER,
errores de funding y disponibilidad de mercado/histórico/análisis. Los contadores
son del proceso y reinician con él. Los errores se registran en logs estructurados;
la auditoría persistente guarda la primera incidencia y cada 60 repeticiones, con
categoría y hora, sin copiar mensajes remotos ni credenciales.

```sh
cargo test --workspace --locked
TEST_DATABASE_URL=postgres://… cargo test --workspace --locked -- --ignored
npm test --prefix web
npm run build --prefix web
cd web
npx playwright install chromium
npm run e2e
DASHBOARD_URL=http://127.0.0.1:3000/dashboard/ npm run e2e -- --grep 'real service'
```

CI comprueba también el dashboard compilado dentro de Docker contra datos públicos
reales, después de las pruebas de arranque y recuperación. Las pruebas de controles
verifican pausa con stops activos, persistencia, reintentos, conflictos de intención,
salidas pendientes sin libro y rechazo de campos no admitidos.

Referencias de las bibliotecas: [Vue](https://vuejs.org/guide/),
[Vuetify](https://vuetifyjs.com/en/getting-started/installation/),
[Pinia](https://pinia.vuejs.org/getting-started),
[ECharts](https://echarts.apache.org/handbook/en/get-started/) y
[Vite](https://vite.dev/guide/). Versiones exactas y dependencias transitivas en
`web/package-lock.json`.
