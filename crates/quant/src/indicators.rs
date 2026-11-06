//! Decimal indicators, SMA seeds; RSI/ATR use Wilder smoothing.
use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
use trading_domain::data::Candle;

fn period(p: usize) -> Result<()> {
    ensure!((2..=200).contains(&p), "indicator period must be 2..200");
    Ok(())
}
fn values(xs: &[D]) -> Result<()> {
    ensure!(
        xs.len() <= 500
            && xs
                .iter()
                .all(|x| x.abs() <= D::from(1_000_000_000_000_000_i64)),
        "indicator numeric bounds exceeded"
    );
    Ok(())
}
pub fn sma(xs: &[D], p: usize) -> Result<Option<D>> {
    period(p)?;
    values(xs)?;
    Ok((xs.len() >= p).then(|| xs[xs.len() - p..].iter().copied().sum::<D>() / D::from(p)))
}
pub fn ema(xs: &[D], p: usize) -> Result<Vec<Option<D>>> {
    period(p)?;
    values(xs)?;
    let mut out = vec![None; xs.len()];
    if xs.len() < p {
        return Ok(out);
    }
    let mut current = xs[..p].iter().copied().sum::<D>() / D::from(p);
    out[p - 1] = Some(current);
    let alpha = D::from(2) / D::from(p + 1);
    for i in p..xs.len() {
        current += alpha * (xs[i] - current);
        out[i] = Some(current);
    }
    Ok(out)
}
pub fn rsi(xs: &[D], p: usize) -> Result<Option<D>> {
    period(p)?;
    values(xs)?;
    if xs.len() <= p {
        return Ok(None);
    }
    let deltas: Vec<_> = xs.windows(2).map(|x| x[1] - x[0]).collect();
    let mut gain = deltas[..p].iter().map(|d| (*d).max(D::ZERO)).sum::<D>() / D::from(p);
    let mut loss = deltas[..p].iter().map(|d| (-*d).max(D::ZERO)).sum::<D>() / D::from(p);
    for d in &deltas[p..] {
        gain = (gain * D::from(p - 1) + (*d).max(D::ZERO)) / D::from(p);
        loss = (loss * D::from(p - 1) + (-*d).max(D::ZERO)) / D::from(p);
    }
    // Ratio form avoids overflow for nearly-zero losses. Flat series is neutral.
    Ok(Some(if gain + loss == D::ZERO {
        D::from(50)
    } else {
        gain / (gain + loss) * D::from(100)
    }))
}
pub fn atr(candles: &[Candle], p: usize) -> Result<Option<D>> {
    period(p)?;
    let highs: Vec<_> = candles.iter().map(|c| c.h).collect();
    values(&highs)?;
    if candles.len() <= p {
        return Ok(None);
    }
    // The first true range needs a previous close, so skip the first candle.
    let ranges: Vec<_> = candles
        .windows(2)
        .map(|c| {
            (c[1].h - c[1].l)
                .max((c[1].h - c[0].c).abs())
                .max((c[1].l - c[0].c).abs())
        })
        .collect();
    values(&ranges)?;
    let mut current = ranges[..p].iter().copied().sum::<D>() / D::from(p);
    for range in &ranges[p..] {
        current = (current * D::from(p - 1) + range) / D::from(p);
    }
    Ok(Some(current))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Macd {
    pub line: D,
    pub signal: D,
    pub histogram: D,
}
pub fn macd(xs: &[D], fast: usize, slow: usize, signal: usize) -> Result<Option<Macd>> {
    ensure!(fast < slow, "MACD fast must be smaller than slow");
    period(signal)?;
    let a = ema(xs, fast)?;
    let b = ema(xs, slow)?;
    let lines: Vec<_> = a
        .iter()
        .zip(&b)
        .filter_map(|(a, b)| a.zip(*b).map(|(a, b)| a - b))
        .collect();
    let smoothed = ema(&lines, signal)?;
    Ok(lines
        .last()
        .zip(smoothed.last().copied().flatten())
        .map(|(line, signal)| Macd {
            line: *line,
            signal,
            histogram: *line - signal,
        }))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub ema_20: Option<D>,
    pub ema_50: Option<D>,
    pub rsi_14: Option<D>,
    pub atr_14: Option<D>,
    pub macd_12_26_9: Option<Macd>,
    pub volume: D,
    pub previous_volume_sma_20: Option<D>,
}
pub fn snapshot(cs: &[Candle]) -> Result<Snapshot> {
    ensure!(!cs.is_empty(), "empty indicator window");
    let closes: Vec<_> = cs.iter().map(|c| c.c).collect();
    let volumes: Vec<_> = cs[..cs.len() - 1].iter().map(|c| c.v).collect();
    Ok(Snapshot {
        ema_20: ema(&closes, 20)?.last().copied().flatten(),
        ema_50: ema(&closes, 50)?.last().copied().flatten(),
        rsi_14: rsi(&closes, 14)?,
        atr_14: atr(cs, 14)?,
        macd_12_26_9: macd(&closes, 12, 26, 9)?,
        volume: cs.last().unwrap().v,
        previous_volume_sma_20: sma(&volumes, 20)?,
    })
}
