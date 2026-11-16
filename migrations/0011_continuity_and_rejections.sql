CREATE TABLE paper_continuity_gaps (
    account_id TEXT NOT NULL REFERENCES paper_accounts,
    previous_tick_ms BIGINT NOT NULL,
    resumed_ms BIGINT NOT NULL,
    PRIMARY KEY(account_id, previous_tick_ms, resumed_ms),
    CHECK (resumed_ms - previous_tick_ms > 60000)
);
-- Historical evidence stays visible after deployment; no replayed fills are invented.
INSERT INTO paper_continuity_gaps
SELECT account_id, previous_tick_ms, observed_at FROM (
    SELECT account_id, observed_at,
           lag(observed_at) OVER (PARTITION BY account_id ORDER BY observed_at) AS previous_tick_ms
    FROM paper_equity
) history WHERE observed_at - previous_tick_ms > 60000 ON CONFLICT DO NOTHING;
CREATE TABLE testnet_signal_rejections (
    account TEXT NOT NULL,
    session_id TEXT NOT NULL,
    close_ms BIGINT NOT NULL,
    observed_ms BIGINT NOT NULL,
    reason TEXT NOT NULL,
    PRIMARY KEY(account, session_id, close_ms),
    FOREIGN KEY(account,session_id) REFERENCES testnet_observer_sessions(account,session_id)
);
