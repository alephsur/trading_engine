<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch, ref } from "vue";
import * as echarts from "echarts/core";
import { CandlestickChart, LineChart } from "echarts/charts";
import {
  GridComponent,
  TooltipComponent,
  DataZoomComponent,
  LegendComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type { EChartsCoreOption } from "echarts/core";
echarts.use([
  CandlestickChart,
  LineChart,
  GridComponent,
  TooltipComponent,
  DataZoomComponent,
  LegendComponent,
  CanvasRenderer,
]);
const props = defineProps<{ option: EChartsCoreOption; label: string }>();
const host = ref<HTMLDivElement>();
let chart: echarts.ECharts | undefined, observer: ResizeObserver | undefined;
onMounted(() => {
  chart = echarts.init(host.value!);
  chart.setOption(props.option);
  observer = new ResizeObserver(() => chart?.resize());
  observer.observe(host.value!);
});
watch(
  () => props.option,
  (value) => chart?.setOption(value, true),
  { deep: true },
);
onBeforeUnmount(() => {
  observer?.disconnect();
  chart?.dispose();
});
</script>
<template>
  <div ref="host" class="chart" role="img" :aria-label="label"></div>
</template>
