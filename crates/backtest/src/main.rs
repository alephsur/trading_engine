use anyhow::{Result, ensure};
use std::{fs::OpenOptions, io::Write, path::Path};
use trading_backtest::{Config, data::Dataset};
fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    ensure!(
        std::fs::metadata(path)?.len() <= 512 * 1024 * 1024,
        "input exceeds 512 MiB"
    );
    Ok(serde_json::from_reader(std::io::BufReader::new(
        std::fs::File::open(path)?,
    ))?)
}
fn write(path: &str, value: &impl serde::Serialize) -> Result<()> {
    // Never overwrite a dataset/report from an earlier experiment.
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(path))?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, data, config, out] if command == "run" => {
            let data: Dataset = read(data)?;
            let config: Config = read(config)?;
            let report = trading_backtest::run(&data, config)?;
            write(out, &report)?;
            println!("{} {}", report.run_id, report.selection_status);
        }
        [command, start, end, out] if command == "export" => {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect(&std::env::var("DATABASE_URL")?)
                .await?;
            let data =
                trading_backtest::export::dataset(&pool, start.parse()?, end.parse()?).await?;
            write(out, &data)?;
            println!("exported {} recorded events", data.records.len());
        }
        _ => anyhow::bail!(
            "usage: trading-backtest run DATASET.json CONFIG.json NEW_REPORT.json | export START_MS END_MS NEW_DATASET.json (DATABASE_URL; migrated database)"
        ),
    }
    Ok(())
}
