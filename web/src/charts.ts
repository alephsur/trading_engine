import type { EChartsCoreOption } from "echarts/core";
import type { Candle, Evaluation } from "./types";
const base = {
  backgroundColor: "transparent",
  textStyle: { color: "#90a3bd", fontFamily: "system-ui" },
  grid: { left: 64, right: 24, top: 36, bottom: 58 },
  tooltip: {
    trigger: "axis",
    backgroundColor: "#17253a",
    borderColor: "#30435c",
    textStyle: { color: "#e7eef8" },
  },
  xAxis: {
    type: "category",
    boundaryGap: true,
    axisLine: { lineStyle: { color: "#2b3a50" } },
    axisLabel: { color: "#90a3bd" },
  },
  yAxis: {
    scale: true,
    splitLine: { lineStyle: { color: "#223047" } },
    axisLabel: { color: "#90a3bd" },
  },
  animation: false,
};
export function candleChart(
  candles: Candle[],
  evaluations: Evaluation[],
): EChartsCoreOption {
  const rows = [...candles].sort((a, b) => a.t - b.t),
    byTime = new Map(evaluations.map((e) => [e.close_time, e.indicators]));
  return {
    ...base,
    legend: {
      top: 0,
      data: ["Precio", "EMA 20", "EMA 50"],
      textStyle: { color: "#a9bad1" },
    },
    xAxis: {
      ...base.xAxis,
      data: rows.map((c) =>
        new Date(c.t).toISOString().slice(5, 16).replace("T", " "),
      ),
    },
    dataZoom: [
      { type: "inside" },
      {
        type: "slider",
        height: 18,
        bottom: 8,
        borderColor: "#283b52",
        textStyle: { color: "#90a3bd" },
      },
    ],
    series: [
      {
        name: "Precio",
        type: "candlestick",
        data: rows.map((c) => [c.o, c.c, c.l, c.h].map(Number)),
        itemStyle: {
          color: "#56debd",
          color0: "#f4878b",
          borderColor: "#56debd",
          borderColor0: "#f4878b",
        },
      },
      ...(["ema_20", "ema_50"] as const).map((key, i) => ({
        name: `EMA ${i ? 50 : 20}`,
        type: "line",
        showSymbol: false,
        connectNulls: false,
        data: rows.map((c) =>
          byTime.get(c.T)?.[key] == null ? null : Number(byTime.get(c.T)![key]),
        ),
        itemStyle: { color: i ? "#e6b878" : "#85a8ff" },
        lineStyle: { width: 1.5, color: i ? "#e6b878" : "#85a8ff" },
      })),
    ],
  };
}
export function equityChart(
  points: { time: number; equity: string | number | null }[],
): EChartsCoreOption {
  const data: (number | null)[][] = [];
  let previous = 0;
  for (const p of points) {
    if (previous && p.time - previous > 90000) data.push([previous + 1, null]);
    data.push([p.time, p.equity == null ? null : Number(p.equity)]);
    previous = p.time;
  }
  return {
    ...base,
    grid: { left: 64, right: 20, top: 20, bottom: 35 },
    xAxis: {
      type: "time",
      splitNumber: 3,
      axisLabel: { color: "#90a3bd", hideOverlap: true },
      axisLine: { lineStyle: { color: "#283b52" } },
    },
    useUTC: true,
    series: [
      {
        type: "line",
        showSymbol: false,
        connectNulls: false,
        data,
        lineStyle: { color: "#56debd", width: 2 },
        areaStyle: { color: "rgba(86,222,189,0.06)" },
      },
    ],
  };
}
