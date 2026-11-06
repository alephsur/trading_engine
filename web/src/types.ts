export type Decimal = string | number;
export interface Position {
  id: number;
  symbol: string;
  long: boolean;
  quantity: Decimal;
  entry: Decimal;
  stop: Decimal;
  take_profit: Decimal;
  closing: boolean;
}
export interface Snapshot {
  mode: string;
  generated_at: number;
  account_current: boolean;
  controls_enabled: boolean;
  account: {
    cash: Decimal;
    equity: Decimal;
    last_tick: number;
    entries_paused: boolean;
    blockers: string[];
    day_start_equity: Decimal;
    positions: Position[];
    config: {
      capital: Decimal;
      max_positions: number;
      daily_loss_fraction: Decimal;
      [key: string]: unknown;
    };
  };
  monitor: null | {
    since: number;
    complete: boolean;
    gap_count: number;
    samples: number;
  };
  metrics: {
    net_return: Decimal | null;
    max_drawdown: Decimal | null;
    observed_max_drawdown: Decimal;
    closed_trades: number;
    win_rate: Decimal | null;
    profit_factor: Decimal | null;
    sharpe_daily: number | null;
    sharpe_status: string;
    trade_metrics_status: string;
    fees: Decimal;
    funding_cash_flow: Decimal;
    ending_equity: Decimal | null;
  };
  positions: {
    position: Position;
    mark: Decimal | null;
    unrealized_pnl: Decimal | null;
    notional: Decimal | null;
  }[];
  equity_curve: { time: number; equity: Decimal | null }[];
  asset_statistics: {
    symbol: string;
    closed_trades: number;
    realized_net: Decimal;
    fees: Decimal;
    funding: Decimal;
  }[];
  orders: Record<string, unknown>[];
  decisions: Record<string, unknown>[];
  events: Record<string, unknown>[];
  errors: Record<string, unknown>[];
  funding: Record<string, unknown>[];
  trade_history: Record<string, unknown>[];
  runtime: Record<string, number | null>;
}
export interface Health {
  status: string;
  database: boolean;
  market_data_fresh: boolean;
  history_ready: boolean;
  analysis_ready: boolean;
  paper_ready: boolean;
}
export interface Candle {
  t: number;
  T: number;
  o: Decimal;
  c: Decimal;
  l: Decimal;
  h: Decimal;
  v: Decimal;
}
export interface Evaluation {
  close_time: number;
  indicators: Record<string, Decimal | null>;
  decisions: { strategy: string; direction: string; reason: string }[];
}
export type Command = {
  command_id: string;
  action: "pause" | "resume" | "close_all" | "close_position";
  position_id?: number;
};
