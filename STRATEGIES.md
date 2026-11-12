# Indicadores y estrategias

## Perfil PAPER activo: ruptura confirmada de una hora

El servidor usa por defecto `confirmed_breakout_1h_v1`, aprobado el 11/11/2026,
en BTC, ETH, SOL y HYPE. Solo registra esta estrategia. Las otras tres frecuencias
siguen capturándose y muestran indicadores, pero devuelven `hold/unsupported_interval`.
El dashboard abre inicialmente el intervalo 1h.

Para una compra deben cumplirse **todas** estas condiciones en una vela cerrada:

1. EMA20 > EMA50 y EMA50 actual > EMA50 de cinco velas atrás.
2. Cierre estrictamente por encima del máximo de las 20 velas anteriores.
3. Volumen ≥1,5 veces la media de las 20 velas anteriores, con media positiva.
4. Cierre > apertura y situado en el 25 % superior de la vela.
5. Rango máximo-mínimo ≤2,5 × ATR14, con ATR y rango positivos.

Para vender, se invierten tendencia y ruptura: EMA20 < EMA50 descendente, cierre
bajo el mínimo anterior, cierre < apertura y situado en el 25 % inferior.
Volumen y límite de rango son los mismos. Canal y volumen medio excluyen la vela
de señal; EMA y ATR la incluyen. Se recalcula todo sobre **exactamente 100 velas**.

La señal se entrega a PAPER después de confirmar el cierre. La entrada depende
del siguiente ciclo elegible y del libro disponible, de su frescura y del riesgo;
no se garantiza el precio de apertura de la hora siguiente. Se conservan sizing,
stops, objetivos, comisiones, funding y límites de `PAPER.md`, incluida la gestión
de posiciones ya abiertas. Cambiar estrategia no reinicia la cuenta.

Configuración reproducible: `config/strategies.confirmed-breakout.json`. El perfil
congela los parámetros contrastados; un cambio en ventana, medias o breakout se
rechaza. Los bloques momentum y mean_reversion se conservan por compatibilidad
del formato, pero no están activos. No se incorporan los filtros adicionales
descartados durante el análisis de cinco agentes.

La simulación aproximada de 180 días obtuvo +3,91 % con spread supuesto de 5 bps
y caída máxima del 2,84 %. Es evidencia histórica con ejecución OHLC aproximada,
no rentabilidad realizada ni garantía futura. Los datos usados para elegir el
patrón no constituyen una nueva prueba independiente de rentabilidad.

## Contrato de cálculo

`trading-quant` es una biblioteca pura: no tiene red, base de datos, reloj propio,
credenciales ni acceso al riesgo. La entrada es una serie de `ClosedCandle`,
construida a partir de velas confirmadas y un tiempo de referencia. Rechaza velas
sin confirmar, futuras, inválidas, series mezcladas, desordenadas o con huecos.

Cada evaluación usa una ventana finita de 100 velas por defecto. Antes de completar
la ventana todas las estrategias devuelven `hold/warmup`, sin emitir señales.
EMA y Wilder se inicializan de nuevo desde el comienzo de esa ventana, de forma
que reiniciar o hacer replay produzca exactamente el mismo resultado sin estado
oculto. **No equivale a una EMA calculada desde un histórico infinito**. El tamaño
forma parte de la identidad de configuración. No se eligen parámetros usando los
resultados futuros ni se optimizan automáticamente.

Los cuatro activos se evalúan por separado. Se conservan evaluaciones de 1m, 5m,
15m y 1h; el perfil activo solo genera señales en 1h. No se combinan intervalos.

## Indicadores

Todos los cálculos usan Decimal y se serializan como strings. Se rechazan precios
o volúmenes mayores de 10^15 en el análisis para mantener las operaciones dentro
de límites numéricos conocidos. Los periodos configurables admiten 2–200.

- **SMA:** media aritmética de las últimas N observaciones.
- **EMA:** semilla SMA de las primeras N observaciones; después alpha = 2/(N+1).
- **RSI:** primeras N variaciones para medias de ganancias/pérdidas, luego Wilder
  (alpha = 1/N). Sin pérdidas devuelve 100; sin ganancias devuelve 0; serie plana
  devuelve 50 por convención explícita. Requiere N+1 cierres.
- **ATR:** true range = máximo de high-low, |high-cierre previo| y |low-cierre previo|.
  Se omite el primer candle por no disponer de cierre previo, semilla SMA y luego
  Wilder. Requiere N+1 velas.
- **MACD:** EMA rápida menos EMA lenta; la línea de señal usa EMA de las diferencias
  disponibles y el histograma es su resta. Ambas EMA de precios tienen su propia
  semilla SMA; la señal se inicializa cuando hay suficientes diferencias.
- **Volumen:** volumen de la vela actual y SMA de volúmenes previos, excluyendo
  siempre la vela actual del umbral contra el que se compara.

La instantánea común expone EMA20/50, RSI14, ATR14, MACD12/26/9 y volumen previo SMA20.
Cada estrategia usa además sus propios parámetros. Por ejemplo, cambiar EMA rápida
no cambia el campo informativo `ema_20`: la configuración exacta queda en el manifiesto.

## Perfil histórico: cuatro estrategias

| Estrategia | Candidato long | Candidato short | Filtros/parámetros por defecto |
| --- | --- | --- | --- |
| EMA Trend Following | Cruce de EMA20 sobre EMA50 | Cruce por debajo | Debe existir el cruce entre los dos últimos cierres |
| Momentum / RSI | Subida en 5 velas ≥10 bps y RSI14 entre 55 y 70 | Caída ≥10 bps y RSI14 entre 30 y 45 | Ambas condiciones obligatorias |
| Breakout | Cierre supera máximos de las 20 velas anteriores | Cierre pierde mínimos anteriores | Volumen ≥1,5 veces SMA20 previa; media previa positiva |
| Mean Reversion | Cierre bajo EMA20−2×ATR14 y RSI14 ≤30 | Cierre sobre EMA20+2×ATR14 y RSI14 ≥70 | ATR debe ser positivo |

`hold` siempre incluye motivo. Momentum, breakout y mean reversion pueden proponer
la misma dirección en cierres sucesivos: cada evento representa una evaluación,
no una instrucción para añadir posiciones. No se supone rentabilidad de estas reglas.

## Configuración y extensibilidad

El servidor usa `Config::confirmed_breakout_1h()` si no se especifica archivo.
`config/strategies.example.json` y `Config::default()` de la biblioteca conservan
el perfil histórico para reproducir backtests y consumidores anteriores. Los JSON
sin `profile` mantienen sus reglas, manifiesto e identidad originales. Elegir ese
archivo explícitamente vuelve a activar las cuatro estrategias históricas.
`STRATEGY_CONFIG_PATH` permite leer un JSON al arrancar; campos desconocidos,
periodos inválidos o umbrales inconsistentes abortan el arranque. No hay endpoints
para editar estrategias ni recarga dinámica. Los cambios deben pasar pruebas y
aprobación explícita antes de usar una configuración nueva.

El manifiesto incluye versión declarada, parámetros, nombres de estrategias y
versión del algoritmo. SHA-256 de su JSON canónico produce `config_id`. Un cambio
crea una identidad nueva y conserva evaluaciones anteriores; no sobrescribe el
histórico ni reutiliza su cursor. Al cambiar la semántica del código debe cambiarse
la versión del algoritmo/estrategia, aunque los parámetros permanezcan iguales.

Una implementación nueva de `Strategy` se registra desde fuera mediante
`Engine::new`. El motor solo conoce la interfaz de lectura y el resultado
`long/short/hold`; no hay excepciones de riesgo ni capacidad de ejecución.

## Persistencia y reanudación

Tras recuperar el histórico, se analizan los cierres hasta `checked_through`.
Se procesan lotes de hasta 500 cierres por serie. El cursor se bloquea en PostgreSQL
para impedir que dos consumidores escriban dos veces. Evaluaciones, señales y
avance de cursor comparten una transacción. Un fallo revierte el lote entero.

Cada evaluación guarda ventana inicial/final, huella de entrada, indicadores,
configuración y decisiones. Las señales direccionales tienen un `event_id`
monótono de base de datos y clave única por configuración, activo, intervalo,
cierre y estrategia. Son eventos internos persistentes. Desde la fase 4 un consumidor
PAPER toma decisiones separadas y auditadas. Los identificadores pueden tener huecos
por rollbacks y confirmarse en distinto orden entre productores.

**Todas las señales llevan `analysis_only:true`**, incluidas las de recuperación
histórica. Los tiempos de creación en DB no convierten una señal antigua en una
entrada actual. El consumidor aplica frescura, riesgo, sizing y reconciliación antes
de simular una orden. Las estrategias no crean directamente órdenes ni posiciones.
Ver [PAPER.md](PAPER.md).

Los endpoints `/strategies`, `/evaluations` y `/signals` muestran la configuración
activa. Las versiones antiguas siguen disponibles en las tablas de auditoría.
`/health` requiere que el análisis alcance la cobertura recuperada; una estrategia
inválida o un fallo de cálculo impide declarar el servicio preparado.

## Verificación de referencia

Los fixtures se generan con `scripts/generate_indicator_fixtures.py` mediante
fracciones exactas y sumas ponderadas, independientes de las recurrencias Rust.
Los tests contrastan EMA, RSI, ATR, MACD y volumen con tolerancia decimal 10^-20.
También cubren señales long/short, filtros, warmup, prefijos sin datos futuros,
replay tras reinicio, rollback y versiones de configuración.

Definiciones consultadas: [guía de indicadores de Fidelity](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide)
y [catálogo TA-Lib](https://ta-lib.org/functions/). Las convenciones de semilla y
serie plana de esta implementación se especifican arriba; no se promete igualdad
con todos los defaults de otras bibliotecas.
