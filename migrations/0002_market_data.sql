CREATE TABLE candles (
    symbol TEXT NOT NULL CHECK (symbol IN ('BTC','ETH','SOL','HYPE')),
    interval TEXT NOT NULL CHECK (interval IN ('1m','5m','15m','1h')),
    open_time BIGINT NOT NULL CHECK (open_time >= 0),
    close_time BIGINT NOT NULL,
    trade_count BIGINT NOT NULL CHECK (trade_count >= 0),
    finalized BOOLEAN NOT NULL DEFAULT FALSE,
    payload JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (symbol, interval, open_time)
);
CREATE TABLE trades (
    symbol TEXT NOT NULL CHECK (symbol IN ('BTC','ETH','SOL','HYPE')),
    exchange_time BIGINT NOT NULL,
    trade_id BIGINT NOT NULL,
    payload JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (symbol, exchange_time, trade_id)
);
CREATE TABLE books (
    symbol TEXT NOT NULL CHECK (symbol IN ('BTC','ETH','SOL','HYPE')),
    exchange_time BIGINT NOT NULL,
    payload JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (symbol, exchange_time)
);
-- Anchor persists across restarts: later snapshots never silently erase a gap.
CREATE TABLE candle_coverage (
    symbol TEXT NOT NULL,
    interval TEXT NOT NULL,
    start_time BIGINT NOT NULL,
    checked_through BIGINT NOT NULL,
    missing_count BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (symbol, interval)
);
