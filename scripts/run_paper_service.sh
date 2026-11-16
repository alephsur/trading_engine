#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
set -a
source .env
set +a
export BIND_ADDR="${PAPER_SERVICE_BIND_ADDR:-127.0.0.1:3001}"
export TRADING_MODE=PAPER
exec target/debug/trading-server
