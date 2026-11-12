import type { Decimal } from "./types";
export const number = (v: Decimal | null | undefined, digits = 2) =>
  v == null || !Number.isFinite(Number(v))
    ? "—"
    : new Intl.NumberFormat("es-ES", {
        minimumFractionDigits: digits,
        maximumFractionDigits: digits,
      }).format(Number(v));
export const money = (v: Decimal | null | undefined) =>
  v == null ? "—" : `${number(v)} USDC`;
export const percent = (v: Decimal | null | undefined) =>
  v == null ? "—" : `${number(Number(v) * 100)} %`;
export const time = (v: number | null | undefined) =>
  v
    ? new Intl.DateTimeFormat("es-ES", {
        timeZone: "UTC",
        month: "short",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      }).format(v) + " UTC"
    : "—";
export const reason = (v: string) =>
  ({
    confirmed_trend_breakout: "Ruptura confirmada a favor de la tendencia",
    unsupported_interval: "Esta estrategia opera solo en velas de una hora",
    excessive_signal_range: "Vela demasiado amplia para entrar",
    trend_filter: "Ruptura sin tendencia confirmada",
    weak_directional_close: "Cierre sin suficiente confirmación",
    zero_volatility: "Volatilidad insuficiente",
    warmup: "Esperando 100 velas cerradas",
    inside_previous_channel: "Precio dentro del rango anterior",
    no_ema_cross: "Sin cruce de medias",
    momentum_or_rsi_filter: "Momentum o RSI fuera de rango",
    volume_filter: "Volumen insuficiente",
    inside_reversion_filters: "Precio dentro del rango",
    exit_unreconciled: "Salida pendiente de conciliación",
    extreme_price_move: "Movimiento de precio extremo",
    operator_paused: "Entradas pausadas",
    funding_unreconciled: "Funding pendiente de conciliación",
    valuation_unavailable: "Valoración no disponible",
    data_not_ready: "Datos no preparados",
    daily_loss: "Límite diario de pérdida",
    daily_loss_limit: "Límite diario de pérdida",
    closing_position: "Salida pendiente",
  })[v] || v.replaceAll("_", " ");
