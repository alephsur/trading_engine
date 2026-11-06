import { defineStore } from "pinia";
import { computed, ref } from "vue";
import type { Snapshot, Health, Candle, Evaluation, Command } from "./types";
export class HttpError extends Error {
  constructor(public status: number) {
    super(`HTTP ${status}`);
  }
}
export async function request<T>(
  path: string,
  init: RequestInit = {},
  allowDegraded = false,
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    signal: AbortSignal.timeout(7000),
    cache: "no-store",
  });
  if (!response.ok && !(allowDegraded && response.status === 503))
    throw new HttpError(response.status);
  return response.json() as Promise<T>;
}
export const useDashboard = defineStore("dashboard", () => {
  const snapshot = ref<Snapshot | null>(null),
    health = ref<Health | null>(null),
    prices = ref<Record<string, string>>({}),
    priceFresh = ref(false);
  const symbol = ref("BTC"),
    interval = ref("5m"),
    candles = ref<Candle[]>([]),
    evaluations = ref<Evaluation[]>([]);
  const strategy = ref<Record<string, unknown> | null>(null),
    coverage = ref<Record<string, unknown> | null>(null);
  const now = ref(Date.now()),
    received = ref(0),
    marketReceived = ref(0),
    errors = ref<string[]>([]),
    marketError = ref(""),
    loading = ref(false);
  const credential = ref(""),
    pending = ref<Command | null>(null),
    sending = ref(false),
    controlMessage = ref("");
  let generation = 0,
    running = false,
    timer: ReturnType<typeof setTimeout> | undefined,
    clock: ReturnType<typeof setInterval> | undefined;
  const fresh = computed(
    () =>
      !!snapshot.value?.account_current &&
      now.value - received.value < 15000 &&
      errors.value.length === 0,
  );
  const marketFresh = computed(
    () =>
      priceFresh.value &&
      now.value - received.value < 15000 &&
      !errors.value.includes("Precios"),
  );
  async function market() {
    const id = ++generation;
    const query = `?symbol=${symbol.value}&interval=${interval.value}&limit=200`;
    marketError.value = "";
    try {
      const [c, e] = await Promise.all([
        request<{ candles: { candle: Candle; closed: boolean }[] }>(
          "/candles" + query,
        ),
        request<{ evaluations: Evaluation[] }>("/evaluations" + query),
      ]);
      if (id !== generation) return;
      candles.value = c.candles
        .filter((row) => row.closed)
        .map((row) => row.candle);
      evaluations.value = e.evaluations;
      marketReceived.value = Date.now();
    } catch {
      if (id === generation)
        marketError.value = "No se pudo actualizar este mercado.";
    }
  }
  async function refresh() {
    if (loading.value) return;
    loading.value = true;
    const tasks = [
      [
        "Cuenta",
        async () => {
          snapshot.value = await request<Snapshot>("/observability");
          received.value = Date.now();
        },
      ],
      [
        "Estado",
        async () => {
          health.value = await request<Health>("/health", {}, true);
        },
      ],
      [
        "Precios",
        async () => {
          const p = await request<{
            prices: Record<string, string>;
            fresh: boolean;
          }>("/prices");
          prices.value = p.prices;
          priceFresh.value = p.fresh;
        },
      ],
      [
        "Estrategias",
        async () => {
          strategy.value =
            await request<Record<string, unknown>>("/strategies");
        },
      ],
      [
        "Cobertura",
        async () => {
          coverage.value =
            await request<Record<string, unknown>>("/data-status");
        },
      ],
    ] as const;
    const results = await Promise.allSettled(tasks.map(([, task]) => task()));
    errors.value = results.flatMap((r, i) =>
      r.status === "rejected" ? [tasks[i]![0]] : [],
    );
    now.value = Date.now();
    loading.value = false;
  }
  async function cycle() {
    await Promise.all([refresh(), market()]);
    if (running) timer = setTimeout(cycle, 5000);
  }
  function start() {
    if (running) return;
    running = true;
    clock = setInterval(() => (now.value = Date.now()), 1000);
    void cycle();
  }
  function stop() {
    running = false;
    generation++;
    clearTimeout(timer);
    clearInterval(clock);
  }
  async function control(action?: Command["action"], position_id?: number) {
    if (sending.value) return;
    if (!pending.value) {
      if (!action) return;
      pending.value = {
        command_id: crypto.randomUUID(),
        action,
        ...(position_id === undefined ? {} : { position_id }),
      };
    }
    sending.value = true;
    controlMessage.value = "Enviando solicitud…";
    try {
      await request("/paper/control", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${credential.value}`,
        },
        body: JSON.stringify(pending.value),
      });
      pending.value = null;
      controlMessage.value =
        "Solicitud confirmada. Los cierres esperan precios y liquidez válidos.";
      await refresh();
    } catch (e) {
      if (
        e instanceof HttpError &&
        [400, 401, 403, 409, 422].includes(e.status)
      ) {
        pending.value = null;
        controlMessage.value =
          e.status === 401
            ? "Clave no válida."
            : "Solicitud rechazada. Actualiza el estado antes de continuar.";
      } else
        controlMessage.value =
          "Confirmación pendiente. Reintenta la misma solicitud para comprobar el resultado.";
    } finally {
      sending.value = false;
    }
  }
  return {
    snapshot,
    health,
    prices,
    priceFresh,
    symbol,
    interval,
    candles,
    evaluations,
    strategy,
    coverage,
    now,
    received,
    marketReceived,
    errors,
    marketError,
    loading,
    credential,
    pending,
    sending,
    controlMessage,
    fresh,
    marketFresh,
    refresh,
    market,
    start,
    stop,
    control,
  };
});
