use anyhow::{Result, ensure};
use trading_live::{AssessmentInput, LiveExecutor};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "status" => {
            println!("LIVE disabled: no production signer or transport in this build")
        }
        [cmd, path] if cmd == "assess" => {
            ensure!(
                std::fs::metadata(path)?.len() <= 1_000_000,
                "assessment exceeds 1 MB"
            );
            let input: AssessmentInput = serde_json::from_slice(&std::fs::read(path)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&trading_live::assess(
                    &input,
                    trading_domain::data::now_ms()
                ))?
            );
        }
        [cmd, ..] if cmd == "submit" => LiveExecutor.submit()?,
        [cmd, ..] if cmd == "cancel" => LiveExecutor.cancel()?,
        _ => anyhow::bail!(
            "usage: trading-live status | assess SNAPSHOT.json; submit/cancel are disabled"
        ),
    }
    Ok(())
}
