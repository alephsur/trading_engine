//! Compare the registered strategy against the retained, independent research filter.
#[path = "support/patterns.rs"]
mod patterns;
use anyhow::{Result, ensure};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};
use trading_domain::{SYMBOLS, data::Candle};
use trading_quant::{ClosedCandle, Engine, config::Config, strategies::Direction};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "usage: verify_confirmed_breakout DATA_DIRECTORY NEW_REPORT.json"
    );
    let legacy = Engine::builtin(Config::default())?;
    let approved = Engine::builtin(Config::confirmed_breakout_1h())?;
    let mut counts = BTreeMap::new();
    for symbol in SYMBOLS {
        let cs: Vec<Candle> = serde_json::from_slice(&fs::read(
            Path::new(&args[1]).join(format!("{symbol}-1h.json")),
        )?)?;
        ensure!(cs.len() >= 100, "missing history");
        let mut signals = 0;
        for window in cs.windows(100) {
            ensure!(
                window.iter().all(|c| c.s == symbol && c.i == "1h"),
                "wrong series"
            );
            let history = window
                .iter()
                .map(|c| ClosedCandle::new(c.clone(), true, window[99].end + 1))
                .collect::<Result<Vec<_>>>()?;
            let original = legacy.evaluate(&history)?;
            let expected = patterns::decisions(window, "trend_breakout", original.decisions)?;
            let actual = approved.evaluate(&history)?;
            ensure!(actual.decisions.len() == 1, "unexpected strategy enabled");
            let direction = expected
                .first()
                .map(|d| d.direction)
                .unwrap_or(Direction::Hold);
            ensure!(
                actual.decisions[0].direction == direction,
                "signal mismatch for {symbol} at {}",
                window[99].t
            );
            signals += usize::from(direction != Direction::Hold);
        }
        counts.insert(
            symbol,
            json!({"windows":cs.len()-99,"signals":signals,"mismatches":0}),
        );
    }
    let report = json!({"legacy_config_id":legacy.id(),"approved_config_id":approved.id(),"comparison":"all rolling 100-candle windows against the retained trend_breakout research filter","symbols":counts,"profitability_validation":"same historical sample; implementation parity only"});
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[2])?;
    serde_json::to_writer_pretty(&mut file, &report)?;
    println!("{report}");
    Ok(())
}
