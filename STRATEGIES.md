# Indicadores y estrategias — fase 3

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

Los cuatro activos se evalúan por separado en 1m, 5m, 15m y 1h. Esta fase no combina
señales de distintos intervalos ni declara confirmación entre timeframes.

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

## Reglas iniciales

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

Valores iniciales en `config/strategies.example.json`, idénticos a los defaults.
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
cierre y estrategia. Son eventos internos persistentes; todavía no tienen un
consumidor de ejecución. Los identificadores pueden tener huecos por rollbacks.

**Todas las señales llevan `analysis_only:true`**, incluidas las de recuperación
histórica. Los tiempos de creación en DB no convierten una señal antigua en una
entrada actual. La fase 4 deberá aplicar frescura, riesgo, sizing y reconciliación.
No se crean órdenes, stops, posiciones ni operaciones simuladas en esta fase.

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
