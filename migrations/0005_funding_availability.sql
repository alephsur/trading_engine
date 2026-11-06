-- Existing rates receive migration time: never pretend an unknown receipt time was the settlement time.
ALTER TABLE funding_rates ADD COLUMN available_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp();
