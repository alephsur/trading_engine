#!/usr/bin/env python3
"""Independent exact-rational reference using weighted sums, not iterative recurrences."""
from fractions import Fraction as F
from decimal import Decimal, localcontext
import json
from pathlib import Path

closes = [10, 11, 10, 12, 11, 14]
highs = [11, 13, 12, 14, 13, 16]
lows = [9, 10, 9, 11, 10, 12]
volumes = [1, 2, 4, 3, 8, 12]


def weighted(values, period, alpha):
    seed = sum(map(F, values[:period])) / period
    tail = values[period:]
    return seed * (1 - alpha) ** len(tail) + sum(
        alpha * F(value) * (1 - alpha) ** (len(tail) - 1 - i)
        for i, value in enumerate(tail)
    )


def ema(values, period):
    return weighted(values, period, F(2, period + 1))


def dec(x):
    with localcontext() as context:
        context.prec = 40
        return str(Decimal(x.numerator) / Decimal(x.denominator))


deltas = [b - a for a, b in zip(closes, closes[1:])]
gain = weighted([max(d, 0) for d in deltas], 3, F(1, 3))
loss = weighted([max(-d, 0) for d in deltas], 3, F(1, 3))
ranges = [max(highs[i] - lows[i], abs(highs[i] - closes[i - 1]), abs(lows[i] - closes[i - 1])) for i in range(1, len(closes))]
macd = [ema(closes[:n], 2) - ema(closes[:n], 3) for n in range(3, len(closes) + 1)]
signal = ema(macd, 2)
fixture = dict(closes=list(map(str, closes)), highs=list(map(str, highs)), lows=list(map(str, lows)), volumes=list(map(str, volumes)), expected={
    "sma_3": dec(F(sum(closes[-3:]), 3)), "ema_3": dec(ema(closes, 3)),
    "rsi_3": dec(100 * gain / (gain + loss)), "atr_3": dec(weighted(ranges, 3, F(1, 3))),
    "macd_line": dec(macd[-1]), "macd_signal": dec(signal), "macd_histogram": dec(macd[-1] - signal),
    "previous_volume_sma_3": dec(F(sum(volumes[-4:-1]), 3)),
})
Path("crates/quant/tests/fixtures/indicators.json").write_text(json.dumps(fixture, indent=2) + "\n")
