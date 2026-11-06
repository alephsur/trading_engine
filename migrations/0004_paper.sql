CREATE TABLE paper_accounts (
    account_id TEXT PRIMARY KEY,
    state JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE paper_decisions (
    account_id TEXT NOT NULL REFERENCES paper_accounts,
    signal_id BIGINT NOT NULL REFERENCES signals(event_id),
    payload JSONB NOT NULL,
    PRIMARY KEY(account_id,signal_id)
);
CREATE TABLE paper_orders (
    account_id TEXT NOT NULL REFERENCES paper_accounts,
    order_id BIGINT NOT NULL,
    payload JSONB NOT NULL,
    PRIMARY KEY(account_id,order_id)
);
CREATE TABLE paper_funding (
    account_id TEXT NOT NULL REFERENCES paper_accounts,
    position_id BIGINT NOT NULL,
    hour BIGINT NOT NULL,
    payload JSONB NOT NULL,
    PRIMARY KEY(account_id,position_id,hour)
);
CREATE TABLE oracle_observations (
    symbol TEXT NOT NULL,
    observed_at BIGINT NOT NULL,
    price TEXT NOT NULL,
    PRIMARY KEY(symbol,observed_at)
);
CREATE TABLE funding_rates (
    symbol TEXT NOT NULL,
    hour BIGINT NOT NULL,
    rate TEXT NOT NULL,
    PRIMARY KEY(symbol,hour)
);
CREATE TABLE paper_events (
    event_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES paper_accounts,
    payload JSONB NOT NULL
);
