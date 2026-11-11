-- Observation only: these records never authorize execution.
CREATE TABLE testnet_observer_sessions (
    account TEXT NOT NULL,
    session_id TEXT NOT NULL,
    config JSONB NOT NULL,
    started_ms BIGINT NOT NULL,
    expires_ms BIGINT NOT NULL,
    paused BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (account, session_id),
    CHECK (expires_ms > started_ms)
);
CREATE TABLE testnet_observations (
    account TEXT NOT NULL,
    session_id TEXT NOT NULL,
    close_ms BIGINT NOT NULL,
    observed_ms BIGINT NOT NULL,
    evaluation JSONB NOT NULL,
    PRIMARY KEY (account, session_id, close_ms),
    FOREIGN KEY (account, session_id)
        REFERENCES testnet_observer_sessions(account, session_id)
);
