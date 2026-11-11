use anyhow::{Result, ensure};
use trading_testnet::{
    client::Client,
    executor,
    model::Intent,
    observer,
    signing::{self, TestnetSigner},
};
use zeroize::Zeroizing;
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("testnet: {error}");
        std::process::exit(1)
    }
}
async fn run() -> Result<()> {
    if std::path::Path::new(".env.testnet").exists() {
        dotenvy::from_filename(".env.testnet")
            .map_err(|_| anyhow::anyhow!("could not load .env.testnet"))?;
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        !args.is_empty(),
        "usage: trading-testnet markets | plan SYMBOL buy|sell | reconcile | watch | check ID intent.json | submit ID intent.json | observe-once ID config.json | observe-run ID config.json | observe-status ID | observe-pause ID"
    );
    let client = Client::new()?;
    if args == ["markets"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&client.markets().await?)?
        );
        return Ok(());
    }
    let account = signing::address(&std::env::var("TESTNET_ACCOUNT").map_err(|_| {
        anyhow::anyhow!("configure TESTNET_ACCOUNT (master account, not API wallet)")
    })?)?;
    if args.len() == 3 && args[0] == "plan" {
        ensure!(
            matches!(args[2].as_str(), "buy" | "sell"),
            "side must be buy or sell"
        );
        let markets = client.markets().await?;
        let state = client.account(&account).await?;
        let book = serde_json::from_value(
            client
                .info(serde_json::json!({"type":"l2Book","coin":args[1]}))
                .await?,
        )?;
        let intent = trading_testnet::model::plan(
            &args[1],
            args[2] == "buy",
            &markets,
            &book,
            &state,
            trading_domain::data::now_ms(),
        )?;
        println!("{}", serde_json::to_string_pretty(&intent)?);
        return Ok(());
    }
    let pool = trading_storage::connect(&std::env::var("DATABASE_URL")?).await?;
    if args.len() == 2 && matches!(args[0].as_str(), "observe-status" | "observe-pause") {
        if args[0] == "observe-pause" {
            observer::pause(&pool, &account, &args[1]).await?;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&observer::status(&pool, &account, &args[1]).await?)?
        );
        return Ok(());
    }
    if args.len() == 3 && matches!(args[0].as_str(), "observe-once" | "observe-run") {
        let bytes = std::fs::read(&args[2])?;
        ensure!(bytes.len() <= 4096, "observer configuration too large");
        let config: observer::Config = serde_json::from_slice(&bytes)?;
        loop {
            // Any invalid data or persistence failure stops this process, without orders.
            let result = observer::tick(&client, &pool, &account, &args[1], &config).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            if args[0] == "observe-once" || result["paused"] == true || result["expired"] == true {
                return Ok(());
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Ok(()),
                _ = tokio::time::sleep(std::time::Duration::from_secs(config.poll_seconds)) => {}
            }
        }
    }
    if args == ["reconcile"] || args == ["watch"] {
        loop {
            match executor::reconcile(&client, &pool, &account).await {
                Ok(value) => println!("{}", serde_json::to_string_pretty(&value)?),
                Err(e) => {
                    if args == ["reconcile"] {
                        return Err(e);
                    }
                    eprintln!("testnet reconciliation unavailable; entries remain blocked");
                }
            }
            if args == ["reconcile"] {
                return Ok(());
            }
            tokio::select! {_ = tokio::signal::ctrl_c()=>return Ok(()),_ = tokio::time::sleep(std::time::Duration::from_secs(10))=>{}}
        }
    }
    ensure!(
        args.len() == 3 && matches!(args[0].as_str(), "check" | "submit"),
        "invalid testnet command"
    );
    let text = std::fs::read(&args[2])?;
    ensure!(text.len() <= 4096, "intent too large");
    let intent: Intent = serde_json::from_slice(&text)?;
    let result = if args[0] == "check" {
        let (action, _) = executor::preflight(&client, &pool, &account, &args[1], &intent).await?;
        serde_json::json!({"network":"TESTNET","unsigned_action":action,"submitted":false})
    } else {
        ensure!(
            std::env::var("TESTNET_CONFIRM").ok().as_deref() == Some("TESTNET_ONLY"),
            "set TESTNET_CONFIRM=TESTNET_ONLY to allow testnet writes"
        );
        let path = std::env::var("TESTNET_PRIVATE_KEY_FILE")
            .map_err(|_| anyhow::anyhow!("configure TESTNET_PRIVATE_KEY_FILE"))?;
        let metadata = std::fs::metadata(&path)
            .map_err(|_| anyhow::anyhow!("testnet key file unavailable"))?;
        ensure!(
            metadata.is_file() && metadata.len() <= 256,
            "invalid testnet key file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                metadata.permissions().mode() & 0o077 == 0,
                "key file must not be accessible by group or others (chmod 600)"
            );
        }
        let key = Zeroizing::new(
            std::fs::read_to_string(path)
                .map_err(|_| anyhow::anyhow!("cannot read testnet key file"))?,
        );
        let signer = TestnetSigner::load(key)?;
        ensure!(
            signer.address() != account,
            "use a dedicated approved API wallet, not the master private key"
        );
        executor::submit(&client, &pool, &signer, &account, &args[1], &intent).await?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
