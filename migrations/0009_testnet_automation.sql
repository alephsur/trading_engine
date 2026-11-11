CREATE TABLE testnet_auto_sessions (
    account TEXT NOT NULL,
    session_id TEXT NOT NULL,
    started_ms BIGINT NOT NULL,
    PRIMARY KEY(account, session_id),
    FOREIGN KEY(account, session_id) REFERENCES testnet_observer_sessions(account, session_id)
);
-- BTC/1m/ema_trend_v1 only. A consumed signal cannot be reused under another session ID.
-- Claiming consumes the entire session budget, even if signing or dispatch later fails.
CREATE TABLE testnet_auto_claims (
    account TEXT NOT NULL,
    session_id TEXT NOT NULL,
    close_ms BIGINT NOT NULL,
    command_id TEXT NOT NULL,
    intent JSONB NOT NULL,
    claimed_ms BIGINT NOT NULL,
    PRIMARY KEY(account, session_id),
    UNIQUE(account, close_ms),
    UNIQUE(account, command_id),
    FOREIGN KEY(account, session_id) REFERENCES testnet_auto_sessions(account, session_id),
    FOREIGN KEY(account, session_id, close_ms) REFERENCES testnet_observations(account, session_id, close_ms)
);
