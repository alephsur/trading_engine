CREATE TABLE paper_commands (
    account_id TEXT NOT NULL REFERENCES paper_accounts,
    command_id TEXT NOT NULL,
    request JSONB NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(account_id,command_id)
);
CREATE TABLE paper_monitor (
    account_id TEXT PRIMARY KEY REFERENCES paper_accounts,
    payload JSONB NOT NULL
);
CREATE TABLE paper_equity (
    account_id TEXT NOT NULL REFERENCES paper_accounts,
    observed_at BIGINT NOT NULL,
    payload JSONB NOT NULL,
    PRIMARY KEY(account_id,observed_at)
);
