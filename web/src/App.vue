<script setup lang="ts">
import {
  defineAsyncComponent,
  computed,
  onMounted,
  onBeforeUnmount,
  ref,
  watch,
} from "vue";
import { useDashboard } from "./store";
import { money, number, percent, time, reason } from "./format";
import { candleChart, equityChart } from "./charts";
const Chart = defineAsyncComponent(() => import("./Chart.vue"));
import type { Command } from "./types";
const s = useDashboard(),
  tab = ref("Resumen"),
  activity = ref("Órdenes"),
  keyDialog = ref(false),
  confirm = ref<Command["action"] | null>(null);
const tabs = ["Resumen", "Mercados", "Actividad", "Sistema"],
  symbols = ["BTC", "ETH", "SOL", "HYPE"];
const snap = computed(() => s.snapshot),
  chart = computed(() => candleChart(s.candles, s.evaluations)),
  curve = computed(() => equityChart(snap.value?.equity_curve || []));
const latestEvaluation = computed(
  () => [...s.evaluations].sort((a, b) => b.close_time - a.close_time)[0],
);
const strategyNames: Record<string, string> = {
  ema_trend_v1: "Tendencia EMA",
  momentum_rsi_v1: "Momentum y RSI",
  breakout_v1: "Ruptura de rango",
  mean_reversion_v1: "Reversión a la media",
};
const activeIndicator = computed(
  () =>
    [...s.evaluations].sort((a, b) => b.close_time - a.close_time)[0]
      ?.indicators,
);
const rows = computed(() => {
  if (!snap.value) return [];
  return (
    {
      Órdenes: snap.value.orders,
      Señales: snap.value.decisions,
      Operaciones: snap.value.trade_history,
      Funding: snap.value.funding,
      Eventos: snap.value.events,
    }[activity.value] || []
  );
});
const canControl = computed(
  () =>
    !!snap.value?.controls_enabled &&
    s.fresh &&
    !!s.credential &&
    !s.sending &&
    !s.pending,
);
watch(
  () => [s.symbol, s.interval],
  () => {
    s.candles = [];
    s.evaluations = [];
    void s.market();
  },
);
onMounted(s.start);
onBeforeUnmount(s.stop);
async function confirmControl() {
  const action = confirm.value;
  confirm.value = null;
  if (action) await s.control(action);
}
</script>
<template>
  <v-app
    ><div class="shell">
      <aside class="sidebar">
        <a class="brand" href="/dashboard/"
          ><span class="brand-symbol">≋</span
          ><span>Trading<span class="brand-light">Engine</span></span></a
        >
        <div class="workspace-label">CENTRO DE OPERACIONES</div>
        <nav aria-label="Principal">
          <button
            v-for="(item, i) in tabs"
            :key="item"
            :class="{ active: tab === item }"
            @click="tab = item"
          >
            <span class="nav-symbol" aria-hidden="true">{{
              ["◫", "⌁", "↔", "◎"][i]
            }}</span
            >{{ item }}
          </button>
        </nav>
        <div class="sidebar-bottom">
          <span class="paper-tag">PAPER</span>
          <p>Mercado real.<br />Operaciones simuladas.</p>
          <div class="environment">Hyperliquid · Perpetuos</div>
        </div>
      </aside>
      <main>
        <header>
          <div>
            <div class="eyebrow">WORKSPACE / {{ tab.toUpperCase() }}</div>
            <h1>{{ tab === "Resumen" ? "Vista general" : tab }}</h1>
            <p class="subtitle">
              {{
                tab === "Resumen"
                  ? "Tu cuenta, el mercado y el riesgo en un solo lugar."
                  : tab === "Mercados"
                    ? "Velas confirmadas e indicadores del motor."
                    : tab === "Actividad"
                      ? "Registro de decisiones y ejecución simulada."
                      : "Disponibilidad, cobertura y configuración del motor."
              }}
            </p>
          </div>
          <div class="header-status">
            <span
              class="status"
              :class="{ good: s.fresh && s.health?.status === 'ready' }"
              ><i></i
              >{{
                s.fresh && s.health?.status === "ready"
                  ? "Sistema operativo"
                  : "Sistema degradado"
              }}</span
            ><small>{{
              s.received ? "Actualizado " + time(s.received) : "Conectando…"
            }}</small>
          </div>
        </header>
        <div v-if="s.errors.length" class="alert danger" role="alert">
          No se pudo actualizar: {{ s.errors.join(", ") }}. Los últimos datos
          pueden estar desactualizados.
        </div>
        <div v-else-if="snap && !s.fresh" class="alert danger" role="alert">
          Cuenta desactualizada. Esperando una valoración reciente antes de
          habilitar los controles.
        </div>
        <div
          v-if="s.health && s.health.status !== 'ready'"
          class="alert"
          role="status"
        >
          El motor está recuperando datos o servicios. Consulta Sistema para ver
          el estado de cada componente.
        </div>
        <div v-if="snap?.account.blockers.length" class="blockers">
          <span v-for="b in snap.account.blockers" :key="b">{{
            reason(b)
          }}</span>
        </div>
        <section v-if="!snap" class="panel loading">
          <h2>
            {{
              s.loading ? "Conectando con el motor…" : "Cuenta no disponible"
            }}
          </h2>
          <p>
            La vista se actualizará automáticamente cuando el servicio responda.
          </p>
          <v-btn variant="outlined" @click="s.refresh">Actualizar</v-btn>
        </section>
        <template v-if="snap">
          <section v-if="tab === 'Resumen'" class="kpis">
            <article class="panel kpi">
              <span>Patrimonio total</span
              ><strong>{{
                money(s.fresh ? snap.metrics.ending_equity : null)
              }}</strong
              ><small
                >Capital inicial {{ money(snap.account.config.capital) }}</small
              >
            </article>
            <article class="panel kpi">
              <span>Rentabilidad acumulada</span
              ><strong
                :class="{
                  positive: s.fresh && Number(snap.metrics.net_return) > 0,
                }"
                >{{ percent(s.fresh ? snap.metrics.net_return : null) }}</strong
              ><small>Incluye comisiones y funding conciliado</small>
            </article>
            <article class="panel kpi">
              <span>Resultado abierto</span
              ><strong>{{
                money(
                  s.fresh && snap.metrics.ending_equity != null
                    ? Number(snap.metrics.ending_equity) -
                        Number(snap.account.cash)
                    : null,
                )
              }}</strong
              ><small>Efectivo {{ money(snap.account.cash) }}</small>
            </article>
            <article class="panel kpi">
              <span>Posiciones abiertas</span
              ><strong
                >{{ snap.positions.length
                }}<em>/ {{ snap.account.config.max_positions }}</em></strong
              ><small>{{
                snap.account.entries_paused
                  ? "Entradas pausadas"
                  : "Entradas habilitadas, sujetas a riesgo"
              }}</small>
            </article>
          </section>
          <section
            v-if="tab === 'Resumen' || tab === 'Mercados'"
            class="markets"
            aria-label="Mercados"
          >
            <button
              v-for="asset in symbols"
              :key="asset"
              class="panel market"
              :class="{ selected: s.symbol === asset }"
              @click="s.symbol = asset"
            >
              <span class="coin" :class="asset.toLowerCase()">{{
                asset.slice(0, 1)
              }}</span
              ><span
                ><b>{{ asset }} <small>/ USDC</small></b
                ><strong>{{ number(s.prices[asset]) }}</strong></span
              ><span
                class="market-fresh"
                :class="{ positive: s.marketFresh }"
                >{{ s.marketFresh ? "EN DIRECTO" : "SIN ACTUALIZAR" }}</span
              >
            </button>
          </section>
          <div
            v-if="tab === 'Resumen' || tab === 'Mercados'"
            class="chart-grid"
          >
            <section class="panel price-panel">
              <div class="panel-heading">
                <div>
                  <h2>
                    {{ s.symbol }} / USDC <span class="subtle">Perpetuo</span>
                  </h2>
                  <p>Velas cerradas · UTC</p>
                </div>
                <div class="intervals" aria-label="Intervalo">
                  <button
                    v-for="i in ['1m', '5m', '15m', '1h']"
                    :key="i"
                    :class="{ selected: s.interval === i }"
                    @click="s.interval = i"
                  >
                    {{ i }}
                  </button>
                </div>
              </div>
              <Chart
                v-if="s.candles.length"
                :option="chart"
                :label="'Velas de ' + s.symbol"
              />
              <div v-else class="chart empty">
                {{ s.marketError || "Esperando velas confirmadas…" }}
              </div>
              <p v-if="s.marketError" class="error-text">{{ s.marketError }}</p>
              <div class="indicator-strip">
                <span
                  v-for="key in ['ema_20', 'ema_50', 'rsi_14', 'atr_14']"
                  :key="key"
                  >{{ key.replace("_", " ").toUpperCase() }}
                  <b>{{ number(activeIndicator?.[key]) }}</b></span
                >
              </div>
            </section>
            <section class="panel safety">
              <div class="panel-heading">
                <h2>Control de riesgo</h2>
                <span class="paper-tag">PAPER</span>
              </div>
              <div class="safety-state">
                <i :class="{ paused: snap.account.entries_paused }"></i>
                <div>
                  <b>{{
                    snap.account.entries_paused
                      ? "Entradas en pausa"
                      : "Motor de simulación activo"
                  }}</b
                  ><small>Stops y salidas permanecen activos</small>
                </div>
              </div>
              <dl>
                <div>
                  <dt>Límite de posiciones</dt>
                  <dd>
                    {{ snap.positions.length }} /
                    {{ snap.account.config.max_positions }}
                  </dd>
                </div>
                <div>
                  <dt>Pérdida diaria máxima</dt>
                  <dd>
                    {{ percent(snap.account.config.daily_loss_fraction) }}
                  </dd>
                </div>
                <div>
                  <dt>Comisiones acumuladas</dt>
                  <dd>{{ money(snap.metrics.fees) }}</dd>
                </div>
                <div>
                  <dt>Funding conciliado</dt>
                  <dd>{{ money(snap.metrics.funding_cash_flow) }}</dd>
                </div>
                <div>
                  <dt>Drawdown observado</dt>
                  <dd>
                    {{
                      percent(
                        snap.monitor
                          ? snap.metrics.observed_max_drawdown
                          : null,
                      )
                    }}
                  </dd>
                </div>
              </dl>
              <p class="footnote">
                Drawdown desde {{ time(snap.monitor?.since) }}.
                {{
                  snap.monitor?.complete
                    ? "Observaciones continuas."
                    : "Cobertura incompleta; puede subestimar la caída."
                }}
              </p>
              <div class="controls">
                <v-btn
                  :disabled="!canControl"
                  color="primary"
                  variant="tonal"
                  @click="
                    confirm = snap.account.entries_paused ? 'resume' : 'pause'
                  "
                  >{{
                    snap.account.entries_paused
                      ? "Iniciar / reanudar"
                      : "Pausar entradas"
                  }}</v-btn
                ><v-btn
                  :disabled="!canControl"
                  variant="outlined"
                  @click="confirm = 'close_all'"
                  >Detener y cerrar posiciones</v-btn
                ><button
                  v-if="snap.controls_enabled"
                  class="text-button"
                  @click="keyDialog = true"
                >
                  {{
                    s.credential
                      ? "Cambiar clave de control"
                      : "Desbloquear controles"
                  }}</button
                ><small v-else>Controles deshabilitados en el servidor.</small
                ><v-btn
                  v-if="s.pending"
                  :loading="s.sending"
                  variant="outlined"
                  @click="s.control()"
                  >Reintentar solicitud pendiente</v-btn
                >
                <p v-if="s.controlMessage" role="status">
                  {{ s.controlMessage }}
                </p>
              </div>
            </section>
          </div>
          <section v-if="tab === 'Mercados'" class="panel">
            <div class="panel-heading">
              <h2>Última evaluación de estrategias</h2>
              <span class="subtle">{{
                time(latestEvaluation?.close_time)
              }}</span>
            </div>
            <div v-if="latestEvaluation" class="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Estrategia</th>
                    <th>Decisión</th>
                    <th>Motivo</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="d in latestEvaluation.decisions" :key="d.strategy">
                    <td>{{ strategyNames[d.strategy] || d.strategy }}</td>
                    <td>
                      {{
                        d.direction === "long"
                          ? "Entrada larga"
                          : d.direction === "short"
                            ? "Entrada corta"
                            : "Esperar"
                      }}
                    </td>
                    <td>{{ reason(d.reason) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p v-else class="subtitle">
              Todavía no hay una evaluación para este intervalo.
            </p>
            <p class="footnote">
              Una señal debe superar los controles de riesgo antes de
              convertirse en una orden PAPER.
            </p>
          </section>
          <section v-if="tab === 'Resumen'" class="panel positions">
            <div class="panel-heading">
              <h2>
                Posiciones abiertas
                <span class="count">{{ snap.positions.length }}</span>
              </h2>
              <span class="subtle">Valoradas al precio de salida</span>
            </div>
            <div v-if="!snap.positions.length" class="empty position-empty">
              <span>◇</span><b>Sin posiciones abiertas</b>
              <p>
                Las señales solo abren posiciones si cumplen todas las reglas de
                riesgo.
              </p>
            </div>
            <div v-else class="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Activo</th>
                    <th>Dirección</th>
                    <th>Cantidad</th>
                    <th>Entrada</th>
                    <th>Marca</th>
                    <th>PnL abierto</th>
                    <th>Stop / Objetivo</th>
                    <th>Estado</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="p in snap.positions" :key="p.position.id">
                    <td>{{ p.position.symbol }}</td>
                    <td>{{ p.position.long ? "Largo" : "Corto" }}</td>
                    <td>{{ number(p.position.quantity, 5) }}</td>
                    <td>{{ number(p.position.entry) }}</td>
                    <td>{{ number(s.fresh ? p.mark : null) }}</td>
                    <td>{{ money(s.fresh ? p.unrealized_pnl : null) }}</td>
                    <td>
                      {{ number(p.position.stop) }} /
                      {{ number(p.position.take_profit) }}
                    </td>
                    <td>
                      <span v-if="p.position.closing">Salida pendiente</span
                      ><v-btn
                        v-else
                        size="small"
                        variant="text"
                        :disabled="!canControl"
                        @click="s.control('close_position', p.position.id)"
                        >Cerrar</v-btn
                      >
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>
          <div v-if="tab === 'Resumen'" class="bottom-grid">
            <section class="panel">
              <div class="panel-heading">
                <h2>Evolución del patrimonio</h2>
                <span class="subtle">Hasta 1440 minutos observados</span>
              </div>
              <Chart
                v-if="snap.equity_curve.length"
                :option="curve"
                label="Evolución del patrimonio PAPER"
              />
              <div v-else class="empty chart">
                Aún no hay observaciones registradas.
              </div>
            </section>
            <section class="panel stats">
              <h2>Rendimiento</h2>
              <dl>
                <div>
                  <dt>Operaciones cerradas</dt>
                  <dd>{{ snap.metrics.closed_trades }}</dd>
                </div>
                <div>
                  <dt>Aciertos</dt>
                  <dd>{{ percent(snap.metrics.win_rate) }}</dd>
                </div>
                <div>
                  <dt>Profit factor</dt>
                  <dd>{{ number(snap.metrics.profit_factor) }}</dd>
                </div>
                <div>
                  <dt>Sharpe diario</dt>
                  <dd>{{ number(snap.metrics.sharpe_daily) }}</dd>
                </div>
              </dl>
              <p class="footnote">
                Aciertos y profit factor requieren 30 operaciones conciliadas.
                Sharpe requiere 30 retornos diarios y cobertura continua. «—»
                indica que no hay una estimación válida.
              </p>
            </section>
          </div>
          <section v-if="tab === 'Resumen'" class="panel">
            <div class="panel-heading">
              <h2>Resultado por activo</h2>
              <span class="subtle">Desde el inicio de la cuenta</span>
            </div>
            <div class="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Activo</th>
                    <th>Cerradas</th>
                    <th>Resultado realizado neto</th>
                    <th>Comisiones</th>
                    <th>Funding</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="a in snap.asset_statistics" :key="a.symbol">
                    <td>{{ a.symbol }}</td>
                    <td>{{ a.closed_trades }}</td>
                    <td>{{ money(a.realized_net) }}</td>
                    <td>{{ money(a.fees) }}</td>
                    <td>{{ money(a.funding) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>
          <section v-if="tab === 'Actividad'" class="panel">
            <div class="panel-heading">
              <h2>Actividad reciente</h2>
              <span class="subtle"
                >Últimos 50 registros · Operaciones: 100</span
              >
            </div>
            <div class="activity-tabs">
              <button
                v-for="item in [
                  'Órdenes',
                  'Señales',
                  'Operaciones',
                  'Funding',
                  'Eventos',
                ]"
                :key="item"
                :class="{ selected: activity === item }"
                @click="activity = item"
              >
                {{ item }}
              </button>
            </div>
            <div v-if="!rows.length" class="empty position-empty">
              No hay registros en esta categoría.
            </div>
            <div v-else class="activity-list">
              <details v-for="(row, index) in rows" :key="index">
                <summary>
                  <b>{{ row.symbol || row.kind || "Cuenta PAPER" }}</b
                  ><span>{{
                    row.strategy || row.status || row.reason || "Registro"
                  }}</span
                  ><time>{{
                    time(
                      Number(row.time || row.close_time || row.opened_at) ||
                        null,
                    )
                  }}</time>
                </summary>
                <pre>{{ JSON.stringify(row, null, 2) }}</pre>
              </details>
            </div>
          </section>
          <template v-if="tab === 'Sistema'"
            ><div class="system-grid">
              <section class="panel">
                <h2>Servicios</h2>
                <dl>
                  <div
                    v-for="(label, key) in {
                      database: 'Base de datos',
                      market_data_fresh: 'Datos de mercado',
                      history_ready: 'Histórico',
                      analysis_ready: 'Análisis',
                      paper_ready: 'Ejecución PAPER',
                    }"
                    :key="key"
                  >
                    <dt>{{ label }}</dt>
                    <dd
                      :class="
                        s.health?.[key] && s.fresh ? 'positive' : 'warning'
                      "
                    >
                      {{
                        s.health?.[key] && s.fresh ? "Disponible" : "Pendiente"
                      }}
                    </dd>
                  </div>
                </dl>
              </section>
              <section class="panel">
                <h2>Telemetría del proceso</h2>
                <dl>
                  <div
                    v-for="(label, key) in {
                      uptime_seconds: 'Tiempo activo (s)',
                      feed_disconnects: 'Desconexiones',
                      paper_errors: 'Fallos PAPER',
                      funding_errors: 'Fallos de funding',
                      feed_age_ms: 'Edad de mercado (ms)',
                      paper_age_ms: 'Edad de cuenta (ms)',
                    }"
                    :key="key"
                  >
                    <dt>{{ label }}</dt>
                    <dd>{{ number(snap.runtime[key], 0) }}</dd>
                  </div>
                </dl>
                <small>Contadores reiniciados al arrancar el proceso.</small>
              </section>
            </div>
            <section class="panel">
              <h2>Alertas registradas</h2>
              <p v-if="!snap.errors.length" class="subtitle">
                Sin fallos registrados.
              </p>
              <pre v-else>{{ JSON.stringify(snap.errors, null, 2) }}</pre>
            </section>
            <section class="panel">
              <h2>Configuración y cobertura</h2>
              <p class="subtitle">
                Parámetros de solo lectura; los cambios de estrategia requieren
                validación.
              </p>
              <details>
                <summary>Estrategias e indicadores</summary>
                <pre>{{ JSON.stringify(s.strategy, null, 2) }}</pre>
              </details>
              <details>
                <summary>Límites de riesgo</summary>
                <pre>{{ JSON.stringify(snap.account.config, null, 2) }}</pre>
              </details>
              <details>
                <summary>Cobertura de velas</summary>
                <pre>{{ JSON.stringify(s.coverage, null, 2) }}</pre>
              </details>
            </section></template
          >
        </template>
        <footer>
          Trading Engine <span>Simulación PAPER · Sin ejecución real</span
          ><span>Datos públicos de Hyperliquid</span>
        </footer>
      </main>
    </div>
    <v-dialog v-model="keyDialog" max-width="440"
      ><v-card title="Desbloquear controles PAPER"
        ><v-card-text
          ><p>
            Introduce la clave configurada para este servidor. Se conserva solo
            en memoria hasta cerrar o recargar esta página.
          </p>
          <v-text-field
            v-model="s.credential"
            label="Clave de control"
            type="password"
            autocomplete="off"
            class="mt-4" /></v-card-text
        ><v-card-actions
          ><v-btn
            @click="
              s.credential = '';
              keyDialog = false;
            "
            >Borrar clave</v-btn
          ><v-btn color="primary" @click="keyDialog = false"
            >Listo</v-btn
          ></v-card-actions
        ></v-card
      ></v-dialog
    >
    <v-dialog
      :model-value="!!confirm"
      max-width="460"
      @update:model-value="confirm = null"
      ><v-card
        :title="
          confirm === 'close_all'
            ? 'Detener simulación'
            : confirm === 'pause'
              ? 'Pausar entradas'
              : 'Reanudar entradas'
        "
        ><v-card-text>{{
          confirm === "close_all"
            ? "Se pausarán las entradas y se solicitará el cierre de todas las posiciones. Una salida puede quedar pendiente si faltan datos o liquidez."
            : confirm === "pause"
              ? "Se bloquearán nuevas entradas. Los stops y las salidas seguirán activos."
              : "Las nuevas entradas volverán a evaluarse con todos los límites de riesgo vigentes."
        }}</v-card-text
        ><v-card-actions
          ><v-btn @click="confirm = null">Cancelar</v-btn
          ><v-btn color="primary" @click="confirmControl"
            >Confirmar</v-btn
          ></v-card-actions
        ></v-card
      ></v-dialog
    >
  </v-app>
</template>
