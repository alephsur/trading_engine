CREATE TABLE events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    received_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    kind TEXT NOT NULL CHECK (kind <> ''),
    payload JSONB NOT NULL
);
CREATE INDEX events_kind_received_at ON events (kind, received_at DESC);
