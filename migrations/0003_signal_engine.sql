CREATE TABLE signal_configs (
    config_id TEXT PRIMARY KEY,
    manifest JSONB NOT NULL,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE signal_cursors (
    config_id TEXT NOT NULL REFERENCES signal_configs,
    symbol TEXT NOT NULL,
    interval TEXT NOT NULL,
    last_open_time BIGINT NOT NULL,
    PRIMARY KEY (config_id,symbol,interval)
);
CREATE TABLE evaluations (
    config_id TEXT NOT NULL REFERENCES signal_configs,
    symbol TEXT NOT NULL,
    interval TEXT NOT NULL,
    open_time BIGINT NOT NULL,
    payload JSONB NOT NULL,
    evaluated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (config_id,symbol,interval,open_time)
);
CREATE TABLE signals (
    event_id BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
    config_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    interval TEXT NOT NULL,
    open_time BIGINT NOT NULL,
    strategy TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('long','short')),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (config_id,symbol,interval,open_time,strategy),
    FOREIGN KEY (config_id,symbol,interval,open_time) REFERENCES evaluations
);
-- A durable internal event, consumed by future phases only after their own risk checks.
CREATE INDEX signals_recent ON signals (config_id,symbol,interval,open_time DESC);
