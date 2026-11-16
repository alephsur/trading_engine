<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { time } from "./format";
type Status = {
  available: boolean;
  connected: boolean;
  active: string | null;
  profiles: { id: string; email: string | null; connected: boolean }[];
  selected_model: string | null;
  last_analysis: null | {
    text: string;
    model: string;
    created_at_ms: number;
    context: { source: string };
  };
};
const state = ref<Status | null>(null),
  models = ref<{ id: string; name: string }[]>([]);
const profile = ref(""),
  model = ref(""),
  busy = ref(false),
  message = ref(""),
  stale = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined,
  stopped = false,
  catalogAccount: string | null = null;
async function api<T>(path: string, body?: unknown): Promise<T> {
  const r = await fetch(`/api/ai/${path}`, {
    method: path === "status" ? "GET" : "POST",
    cache: "no-store",
    headers: {
      "Content-Type": "application/json",
      "X-Trading-Client": "dashboard",
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal: AbortSignal.timeout(path === "status" ? 7000 : 100000),
  });
  const value = await r.json();
  if (!r.ok)
    throw new Error(value.error || "No se pudo completar la solicitud.");
  return value as T;
}
async function refresh() {
  try {
    const value = await api<Status>("status");
    const changed =
      state.value?.active !== value.active ||
      state.value?.selected_model !== value.selected_model;
    state.value = value;
    stale.value = false;
    if (changed) model.value = value.selected_model || "";
    if (value.connected && value.active !== catalogAccount && !busy.value) {
      models.value = [];
      models.value = (
        await api<{ models: { id: string; name: string }[] }>("models")
      ).models;
      catalogAccount = value.active;
    }
  } catch {
    stale.value = true;
  }
}
async function cycle() {
  if (!busy.value) await refresh();
  if (!stopped) timer = setTimeout(cycle, 5000);
}
onMounted(() => void cycle());
onUnmounted(() => {
  stopped = true;
  clearTimeout(timer);
});
async function connect() {
  const popup = window.open("about:blank", "_blank");
  if (!popup) {
    message.value = "Permite abrir una pestaña para iniciar sesión en OpenAI.";
    return;
  }
  popup.opener = null;
  busy.value = true;
  message.value = "";
  try {
    const result = await api<{ url: string }>("connect", {
      profile: profile.value || null,
    });
    const url = new URL(result.url);
    if (url.origin !== "https://auth.openai.com")
      throw new Error("Destino de autorización no válido.");
    popup.location.href = result.url;
    message.value =
      "Completa la autorización en OpenAI y vuelve aquí. La conexión se actualizará automáticamente.";
  } catch (e) {
    popup.close();
    message.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function act(kind: "model" | "analyze" | "disconnect") {
  busy.value = true;
  message.value = "";
  try {
    const result = await api<{ remote_revocation_confirmed?: boolean }>(
      kind,
      kind === "model" ? { model: model.value } : undefined,
    );
    message.value =
      kind === "disconnect"
        ? result.remote_revocation_confirmed
          ? "Sesión desconectada."
          : "Sesión eliminada localmente. No se confirmó la revocación remota; desconecta también la aplicación en los ajustes de ChatGPT."
        : kind === "model"
          ? "Modelo guardado."
          : "Análisis completado. No se han enviado órdenes.";
    if (kind === "disconnect") {
      catalogAccount = null;
      models.value = [];
    }
    await refresh();
  } catch (e) {
    message.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="panel" aria-labelledby="chatgpt-title">
    <h2 id="chatgpt-title">Asistente ChatGPT</h2>
    <p class="subtitle">
      Análisis consultivo. No abre operaciones ni modifica la estrategia o los
      límites de riesgo.
    </p>
    <p v-if="stale" role="status">
      No se pudo actualizar la conexión. Comprueba el servicio local.
    </p>
    <p v-if="state && !state.available">
      La conexión con ChatGPT no está habilitada en este servidor.
    </p>
    <template v-if="state?.available">
      <p>
        {{ state.connected ? "Cuenta conectada" : "Sin cuenta conectada"
        }}<span v-if="state.connected">
          ·
          {{
            state.profiles.find((p) => p.id === state?.active)?.email ||
            "Cuenta autorizada"
          }}</span
        >
      </p>
      <p class="subtitle">
        Al conectar, autorizas el uso de tu plan de ChatGPT. La prueba envía
        únicamente precios públicos, su frescura y el identificador de
        estrategia; no envía claves ni datos de tu cuenta del exchange.
      </p>
      <label
        >Cuenta para iniciar sesión
        <select v-model="profile" :disabled="busy">
          <option value="">Añadir cuenta de ChatGPT</option>
          <option v-for="p in state.profiles" :key="p.id" :value="p.id">
            {{ p.email || "Registro pendiente" }} · {{ p.id.slice(-8) }}
          </option>
        </select>
      </label>
      <v-btn variant="outlined" :disabled="busy || stale" @click="connect"
        >Continuar con ChatGPT</v-btn
      >
      <v-btn
        v-if="state.connected"
        variant="text"
        :disabled="busy"
        @click="act('disconnect')"
        >Desconectar ChatGPT</v-btn
      >
      <div v-if="state.connected" class="ai-model">
        <label
          >Modelo disponible en tu cuenta
          <select v-model="model" :disabled="busy || stale">
            <option value="" disabled>Selecciona un modelo</option>
            <option v-for="m in models" :key="m.id" :value="m.id">
              {{ m.name }}
            </option>
          </select>
        </label>
        <v-btn
          variant="outlined"
          :disabled="busy || stale || !model"
          @click="act('model')"
          >Guardar modelo</v-btn
        >
        <v-btn
          :disabled="busy || stale || !state.selected_model"
          @click="act('analyze')"
          >Probar análisis</v-btn
        >
        <p class="subtitle">
          Esta prueba usa el feed público del dashboard. La conexión automática
          a señales testnet se realizará después de validar el acceso.
        </p>
      </div>
      <p v-if="busy" role="status">
        Procesando solicitud… Puede tardar hasta 90 segundos.
      </p>
      <p v-if="message" role="status">{{ message }}</p>
      <p>
        <a
          href="https://chatgpt.com/settings/usage"
          target="_blank"
          rel="noopener noreferrer"
          >Consultar uso y límites en ChatGPT</a
        >
      </p>
      <article v-if="state.last_analysis">
        <h3>Último análisis guardado</h3>
        <p class="subtitle">
          {{ state.last_analysis.model }} ·
          {{ time(state.last_analysis.created_at_ms) }} · Datos públicos, no
          testnet
        </p>
        <p class="ai-answer">{{ state.last_analysis.text }}</p>
      </article>
    </template>
  </section>
</template>
<style scoped>
label {
  display: block;
  margin: 16px 0;
}
select {
  display: block;
  width: 100%;
  max-width: 520px;
  padding: 10px;
  border: 1px solid #53657b;
  border-radius: 6px;
  color: #e1e9f3;
  background: #152132;
  margin-top: 8px;
}
.ai-answer {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.ai-model {
  margin-top: 20px;
}
a {
  color: #6bd5c1;
}
</style>
