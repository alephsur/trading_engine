-- Preserve existing BTC/1m claims, while deduplicating each newly supported series.
ALTER TABLE testnet_auto_claims
    ADD COLUMN symbol TEXT NOT NULL DEFAULT 'BTC',
    ADD COLUMN interval TEXT NOT NULL DEFAULT '1m',
    ADD COLUMN strategy TEXT NOT NULL DEFAULT 'ema_trend_v1';
ALTER TABLE testnet_auto_claims DROP CONSTRAINT testnet_auto_claims_account_close_ms_key;
ALTER TABLE testnet_auto_claims ADD UNIQUE(account,symbol,interval,strategy,close_ms);

-- Opt-in account-wide entry gate. Failure/absence of a recent cycle blocks entries.
CREATE TABLE testnet_supervision (
    account TEXT PRIMARY KEY,
    observed_ms BIGINT NOT NULL DEFAULT 0,
    blocked BOOLEAN NOT NULL DEFAULT TRUE,
    funding_through_ms BIGINT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{"blockers":["monitor_starting"]}'
);
CREATE TABLE testnet_funding (
    account TEXT NOT NULL,
    time_ms BIGINT NOT NULL,
    symbol TEXT NOT NULL,
    payload JSONB NOT NULL,
    PRIMARY KEY(account,time_ms,symbol)
);
