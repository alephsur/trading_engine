-- Independent from PAPER. Signed requests are short-lived and never exposed by HTTP.
CREATE TABLE testnet_signers (
    signer TEXT PRIMARY KEY,
    account TEXT NOT NULL,
    nonce BIGINT NOT NULL
);
CREATE TABLE testnet_requests (
    account TEXT NOT NULL,
    command_id TEXT NOT NULL,
    signer TEXT NOT NULL REFERENCES testnet_signers,
    nonce BIGINT NOT NULL,
    created_ms BIGINT NOT NULL,
    expires_ms BIGINT NOT NULL,
    intent JSONB NOT NULL,
    signed_request JSONB NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('prepared','uncertain','acknowledged','rejected','expired')),
    response JSONB,
    reconciliation JSONB,
    PRIMARY KEY (account,command_id),
    UNIQUE (signer,nonce)
);
CREATE TABLE testnet_fills (
    account TEXT NOT NULL,
    oid BIGINT NOT NULL,
    tid BIGINT NOT NULL,
    payload JSONB NOT NULL,
    PRIMARY KEY (account,oid,tid)
);
CREATE TABLE testnet_snapshots (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    account TEXT NOT NULL,
    observed_ms BIGINT NOT NULL,
    payload JSONB NOT NULL
);
CREATE INDEX testnet_requests_pending ON testnet_requests(account,created_ms);
CREATE TABLE testnet_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    account TEXT NOT NULL,
    command_id TEXT NOT NULL,
    observed_ms BIGINT NOT NULL,
    kind TEXT NOT NULL,
    payload JSONB NOT NULL
);
CREATE TABLE testnet_days (
    account TEXT NOT NULL,
    day BIGINT NOT NULL,
    initial_equity TEXT NOT NULL,
    loss_latched BOOLEAN NOT NULL DEFAULT FALSE,
    entry_attempts INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account,day)
);
