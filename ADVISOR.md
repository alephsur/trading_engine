# Análisis supervisado — fase 9

`trading-advisor` es un proceso offline separado. Recibe contexto y una respuesta
estructurada de análisis, conserva su procedencia declarada, ejecuta backtesting
y exige revisión antes de exportar parámetros para estudio en PAPER. No accede al
servidor, no envía órdenes, no modifica riesgo ni recarga estrategias en ejecución.

## Proveedor y límites de confianza

La interfaz Rust `Advisor` admite un proveedor de análisis; esta entrega incluye
`ImportedAdvice` para respuestas JSON obtenidas externamente. **No hay un LLM
conectado ni llamadas a un proveedor**. No se han generado noticias, recomendaciones
ni resultados de modelo simulados como si fueran reales. `model` es procedencia
declarada, no una certificación del proveedor. Para conectar un servicio concreto
hay que seleccionar modelo, credenciales y política de datos en un trabajo posterior.

El contexto contiene símbolo, fecha de disponibilidad y observaciones con fuente:
pueden ser noticias, explicaciones de indicadores/señales o operaciones anteriores.
Su veracidad y completitud dependen del productor de datos. El prompt declara el
contenido como datos no confiables; nunca se interpreta como instrucciones de shell,
rutas, órdenes o aprobación. Los campos JSON desconocidos se rechazan. Los textos
se limitan en tamaño; los parámetros propuestos usan la validación de `trading-quant`.

## Flujo reproducible

1. Preparar un `Context` con el formato de `config/advisor-context.example.json`,
   usando fuentes reales y tiempos de disponibilidad conocidos para investigación.
   La muestra incluida es sintética y sirve únicamente para verificar el formato.
2. Generar el prompt versionado (el archivo de salida debe ser nuevo):

   ```sh
   cargo run --locked -p trading-advisor -- prompt contexto.json prompt.json
   ```

3. Obtener externamente una respuesta JSON con los siguientes campos exactos:

   ```json
   {
     "context_hash": "COPIAR_EL_HASH_DEL_PROMPT",
     "model": "proveedor/modelo/version",
     "prompt_version": "advisory-v1",
     "regime": "unknown",
     "summary": "Conclusiones y limitaciones del análisis",
     "signal_explanation": "Explicación de señales, si existen",
     "post_trade_analysis": "Análisis posterior, si procede",
     "proposed_strategy": null
   }
   ```

   `regime` admite `trending`, `ranging`, `volatile`, `unknown`. Para investigar un
   cambio, `proposed_strategy` contiene un objeto completo con el mismo esquema de
   `config/strategies.example.json`, incluida una versión identificable. No admite
   riesgo, órdenes, claves, herramientas ni aprobación. `null` permite análisis sin
   propuesta; en ese caso no se habilita evaluación/exportación de parámetros.

4. Registrar una propuesta inmutable con huella de contexto y respuesta:

   ```sh
   cargo run --locked -p trading-advisor -- propose contexto.json respuesta.json propuesta.json
   ```

5. El operador prepara dataset y experimento según `BACKTEST.md`. El experimento
   controla periodos, costes, latencia y riesgo; el analista solo aporta parámetros
   de estrategia. El evaluador valida el experimento original y ejecuta una única
   candidata: la propuesta. No afirma haber ganado una comparación con el baseline.

   ```sh
   cargo run --locked -p trading-advisor -- evaluate propuesta.json datos.json experimento.json 30 evidencia.json
   ```

   `30` es el mínimo de operaciones cerradas exigido por el operador en **cada**
   tramo validation/test. Debe ser 1–1000. Training conserva su mínimo propio.
   El contexto debe ser anterior a validation; se exigen cobertura y valoración
   completas. La elegibilidad habilita revisión, no declara rentabilidad, significación
   estadística ni aptitud para producción. El riesgo del experimento se conserva.
   Los resultados incluyen costes y las limitaciones del motor de backtesting.

6. Revisar manualmente `evidencia.json`: datos, muestra, costes, drawdown, operaciones
   abiertas al final, diferencias train/validation/test y sensibilidad. Si se decide
   continuar el estudio, ejecutar desde una terminal interactiva:

   ```sh
   cargo run --locked -p trading-advisor -- approve propuesta.json datos.json experimento.json 30 NOMBRE_REVISOR aprobacion.json
   ```

   El programa vuelve a calcular el backtest y requiere escribir `APPROVE` seguido
   del hash mostrado. Un proceso sin terminal no puede usar este comando. No hay
   opción `--yes` ni aprobación dentro del JSON del modelo.

7. Exportar a un archivo nuevo, sin desplegar ni sobrescribir configuración:

   ```sh
   cargo run --locked -p trading-advisor -- export propuesta.json datos.json experimento.json 30 aprobacion.json estrategia-revisada.json
   ```

   La exportación vuelve a calcular la evidencia y valida la revisión contra esa
   huella exacta. Cambios de propuesta, datos, riesgo, periodos, umbral o resultados
   invalidan la aprobación. El resultado contiene únicamente `StrategyConfig`.
   Aplicarlo a PAPER es una acción posterior explícita del operador; no se cambia
   `STRATEGY_CONFIG_PATH`, el motor, la DB ni un endpoint desde este flujo.

## Auditoría y límites

Los archivos de propuesta/evidencia/revisión se crean sin sobrescribir anteriores.
La revisión registra revisor, fecha y hashes. Son artefactos locales del operador,
**no firmas digitales ni control de acceso multiusuario**: quien pueda modificar
los archivos puede falsificar una revisión. Mantener el directorio de aprobación
fuera del alcance de procesos no confiables; el adaptador de análisis no recibe
rutas ni funciones de aprobación. El rechazo de entrada JSON y la revisión
interactiva no sustituyen aislamiento de permisos en un despliegue multiusuario.

Un hash no demuestra que el modelo nunca haya visto el holdout. Conservar conjuntos
reservados, registrar versiones y evitar seleccionar parámetros retrospectivamente
mediante muchas ejecuciones. Las fechas declaradas requieren fuentes confiables.

`scripts/advisor_smoke.py` verifica el circuito con datos y una revisión sintéticos
en un directorio temporal. No llama a un LLM, no emite una aprobación humana real
y no despliega la configuración exportada.
