import type { Snapshot } from "../src/types";
export function snapshot(): Snapshot {
  const now = Date.now();
  return {
    mode: "PAPER",
    generated_at: now,
    account_current: true,
    controls_enabled: true,
    account: {
      cash: "10000",
      equity: "10000",
      last_tick: now,
      entries_paused: false,
      blockers: [],
      day_start_equity: "10000",
      positions: [],
      config: {
        capital: "10000",
        max_positions: 2,
        daily_loss_fraction: "0.01",
      },
    },
    monitor: {
      since: now - 3600000,
      complete: true,
      gap_count: 0,
      samples: 100,
    },
    metrics: {
      net_return: "0",
      max_drawdown: "0",
      observed_max_drawdown: "0",
      closed_trades: 0,
      win_rate: null,
      profit_factor: null,
      sharpe_daily: null,
      sharpe_status: "insufficient_daily_returns",
      trade_metrics_status: "insufficient_trades",
      fees: "0",
      funding_cash_flow: "0",
      ending_equity: "10000",
    },
    positions: [],
    equity_curve: [
      { time: now - 60000, equity: "10000" },
      { time: now, equity: "10000" },
    ],
    asset_statistics: ["BTC", "ETH", "SOL", "HYPE"].map((symbol) => ({
      symbol,
      closed_trades: 0,
      realized_net: "0",
      fees: "0",
      funding: "0",
    })),
    orders: [],
    decisions: [],
    events: [],
    errors: [],
    funding: [],
    trade_history: [],
    runtime: {
      uptime_seconds: 3600,
      feed_disconnects: 0,
      paper_errors: 0,
      funding_errors: 0,
      feed_age_ms: 150,
      paper_age_ms: 100,
    },
  };
}
export function candles() {
  return Array.from({ length: 60 }, (_, i) => ({
    t: Date.now() - (60 - i) * 300000,
    T: Date.now() - (59 - i) * 300000 - 1,
    o: String(72000 + i * 5 + Math.sin(i) * 50),
    c: String(72010 + i * 5 + Math.sin(i + 1) * 50),
    l: String(71900 + i * 5),
    h: String(72100 + i * 5),
    v: "100",
  }));
}
