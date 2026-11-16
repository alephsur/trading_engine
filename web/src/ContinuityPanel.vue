<script setup lang="ts">
import { computed } from "vue";
import { useDashboard } from "./store";
import { number, time } from "./format";
const s = useDashboard();
const status = computed(() => {
  if (!s.continuityFresh || s.continuityTickAge == null)
    return "Sin actualización reciente";
  if (s.continuityTickAge > s.continuity!.gap_threshold_ms)
    return "Interrupción en curso";
  if (s.continuityTickAge >= 15000) return "Esperando actividad reciente";
  return "Motor activo";
});
const hours = (ms: number) => `${number(ms / 3600000)} h`;
</script>

<template>
  <section class="panel" aria-labelledby="continuity-title">
    <h2 id="continuity-title">Continuidad PAPER</h2>
    <p role="status">{{ status }}</p>
    <p v-if="!s.continuityFresh" class="subtitle">
      No se puede confirmar la continuidad actual. Los datos anteriores, si
      existen, se conservan como referencia.
    </p>
    <template v-if="s.continuity">
      <dl>
        <div>
          <dt>Última actividad registrada</dt>
          <dd>{{ time(s.continuity.last_tick_ms) }}</dd>
        </div>
        <div>
          <dt>Interrupciones registradas</dt>
          <dd>{{ number(s.continuity.gap_count, 0) }}</dd>
        </div>
        <div>
          <dt>Tiempo acumulado de interrupciones cerradas</dt>
          <dd>{{ hours(s.continuity.recorded_gap_ms) }}</dd>
        </div>
      </dl>
      <p class="subtitle">
        Se registran huecos de más de
        {{ number(s.continuity.gap_threshold_ms / 1000, 0) }} segundos.
        Recuperar velas no recupera la observación en tiempo real. La actividad
        del motor tampoco garantiza que el feed y las valoraciones estuvieran
        disponibles.
      </p>
      <p v-if="!s.continuity.gap_count">
        Sin interrupciones registradas por encima del umbral.
      </p>
      <template v-else>
        <p>
          Últimas {{ s.continuity.recent_gaps.length }} de
          {{ s.continuity.gap_count }} interrupciones. Horarios UTC.
        </p>
        <div class="table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Última actividad</th>
                <th scope="col">Reanudación</th>
                <th scope="col">Duración</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="gap in s.continuity.recent_gaps"
                :key="`${gap.previous_tick_ms}-${gap.resumed_ms}`"
              >
                <td>{{ time(gap.previous_tick_ms) }}</td>
                <td>{{ time(gap.resumed_ms) }}</td>
                <td>{{ hours(gap.duration_ms) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </template>
    <button class="text-button" @click="s.refreshContinuity">
      Actualizar continuidad
    </button>
  </section>
</template>
