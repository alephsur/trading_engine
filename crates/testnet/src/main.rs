use anyhow::{Result, ensure};
use trading_testnet::{
    automation,
    client::Client,
    executor,
    model::Intent,
    observer,
    runtime::{Shutdown, SupervisorLock},
    signing::{self, TestnetSigner},
    supervision,
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
        "usage: trading-testnet markets | plan SYMBOL buy|sell | reconcile | watch | check ID intent.json | submit ID intent.json | observe-once ID config.json | observe-run ID config.json | observe-status ID | observe-pause ID | auto-once ID config.json | auto-run ID config.json | auto-status ID | auto-pause ID | supervise-once | supervise-run | supervise-status"
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
    if args.len() == 2 && args[0] == "probe-run" {
        ensure!(
            std::env::var("TESTNET_CONFIRM").ok().as_deref() == Some("TESTNET_ONLY")
                && std::env::var("TESTNET_PROBE_CONFIRM").ok().as_deref()
                    == Some("SYNTHETIC_ONE_ENTRY"),
            "probe requires TESTNET_ONLY and SYNTHETIC_ONE_ENTRY confirmations"
        );
        let mut shutdown = Shutdown::install()?;
        let mut owner = SupervisorLock::acquire(&pool, &account).await?;
        let signer = load_signer(&account)?;
        let report = trading_testnet::probe::run(
            &client,
            &pool,
            &signer,
            &account,
            &args[1],
            &mut owner,
            &mut shutdown,
        )
        .await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if args == ["supervise-status"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&supervision::status(&pool, &account).await?)?
        );
        return Ok(());
    }
    if args == ["supervise-once"] || args == ["supervise-run"] {
        let mut shutdown = Shutdown::install()?;
        let mut owner = SupervisorLock::acquire(&pool, &account).await?;
        loop {
            if shutdown.requested() {
                supervision::failed(&pool, &account, "monitor_stopped").await?;
                return Ok(());
            }
            if let Err(error) = owner.check().await {
                let _ = supervision::failed(&pool, &account, "monitor_ownership_lost").await;
                return Err(error);
            }
            match supervision::tick(&client, &pool, &account).await {
                Ok(value) => println!("{}", serde_json::to_string_pretty(&value["assessment"])?),
                Err(error) if args == ["supervise-once"] => return Err(error),
                Err(error) => {
                    eprintln!(
                        "testnet supervisor unavailable; new entries blocked; retrying reads: {}",
                        trading_testnet::diagnostics::describe(&error)
                    )
                }
            }
            if shutdown.requested() {
                supervision::failed(&pool, &account, "monitor_stopped").await?;
                return Ok(());
            }
            if args == ["supervise-once"] {
                return Ok(());
            }
            tokio::select! {
                _ = shutdown.wait() => {
                    supervision::failed(&pool, &account, "monitor_stopped").await?;
                    return Ok(());
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {}
            }
        }
    }
    if args.len() == 2 && matches!(args[0].as_str(), "auto-status" | "auto-pause") {
        if args[0] == "auto-pause" {
            observer::pause(&pool, &account, &args[1]).await?;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&automation::status(&pool, &account, &args[1]).await?)?
        );
        return Ok(());
    }
    if args.len() == 3 && matches!(args[0].as_str(), "auto-once" | "auto-run") {
        let bytes = std::fs::read(&args[2])?;
        ensure!(bytes.len() <= 4096, "automatic configuration too large");
        let config: observer::Config = serde_json::from_slice(&bytes)?;
        config.validate()?;
        let supervised = config.strategy == "confirmed_breakout_1h_v1";
        ensure!(
            !supervised || args[0] == "auto-run",
            "confirmed breakout requires auto-run so monitoring continues after entry; use observe-once for read-only analysis"
        );
        let confirmation = if supervised {
            "CONFIRMED_BREAKOUT_ONE_ENTRY"
        } else {
            "BTC_EMA_ONE_ENTRY"
        };
        ensure!(
            std::env::var("TESTNET_CONFIRM").ok().as_deref() == Some("TESTNET_ONLY")
                && std::env::var("TESTNET_AUTO_CONFIRM").ok().as_deref() == Some(confirmation),
            "automatic testnet requires TESTNET_CONFIRM=TESTNET_ONLY and TESTNET_AUTO_CONFIRM={confirmation}"
        );
        let mut shutdown = Shutdown::install()?;
        let mut owner = SupervisorLock::acquire(&pool, &account).await?;
        let signer = load_signer(&account)?;
        loop {
            if shutdown.requested() {
                stop_automatic(&pool, &account, &args[1], supervised).await?;
                return Ok(());
            }
            if let Err(error) = owner.check().await {
                let _ = stop_automatic(&pool, &account, &args[1], supervised).await;
                return Err(error);
            }
            let result = match automation::tick(
                &client, &pool, &signer, &account, &args[1], &config,
            )
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    // Never resume automatic entries after an operational error without a new session.
                    let _ = observer::pause(&pool, &account, &args[1]).await;
                    if supervised {
                        let _ = supervision::failed_with_error(
                            &pool,
                            &account,
                            "automatic_cycle_failed",
                            &error,
                        )
                        .await;
                        if args[0] == "auto-run" {
                            eprintln!(
                                "automatic testnet paused; continuing position supervision: {}",
                                trading_testnet::diagnostics::describe(&error)
                            );
                            tokio::select! {
                                _ = shutdown.wait() => {
                                    stop_automatic(&pool, &account, &args[1], supervised).await?;
                                    return Ok(());
                                },
                                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {}
                            }
                            continue;
                        }
                    }
                    return Err(error);
                }
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
            if shutdown.requested() {
                stop_automatic(&pool, &account, &args[1], supervised).await?;
                return Ok(());
            }
            let finished = result["paused"] == true
                || result["expired"] == true
                || result["entry_budget_consumed"] == true;
            if args[0] == "auto-once"
                || (finished
                    && (!supervised || result["supervision"]["assessment"]["safe_to_stop"] == true))
            {
                return Ok(());
            }
            tokio::select! {
                _ = shutdown.wait() => {
                    stop_automatic(&pool, &account, &args[1], supervised).await?;
                    return Ok(());
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(config.poll_seconds)) => {}
            }
        }
    }
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
        let mut shutdown = Shutdown::install()?;
        loop {
            if shutdown.requested() {
                return Ok(());
            }
            // Any invalid data or persistence failure stops this process, without orders.
            let result = observer::tick(&client, &pool, &account, &args[1], &config).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            if args[0] == "observe-once" || result["paused"] == true || result["expired"] == true {
                return Ok(());
            }
            tokio::select! {
                _ = shutdown.wait() => return Ok(()),
                _ = tokio::time::sleep(std::time::Duration::from_secs(config.poll_seconds)) => {}
            }
        }
    }
    if args == ["reconcile"] || args == ["watch"] {
        let mut shutdown = Shutdown::install()?;
        loop {
            if shutdown.requested() {
                return Ok(());
            }
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
            tokio::select! {_ = shutdown.wait()=>return Ok(()),_ = tokio::time::sleep(std::time::Duration::from_secs(10))=>{}}
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
        let signer = load_signer(&account)?;
        executor::submit(&client, &pool, &signer, &account, &args[1], &intent).await?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn stop_automatic(
    pool: &sqlx::PgPool,
    account: &str,
    id: &str,
    supervised: bool,
) -> Result<()> {
    if supervised {
        // One transaction blocks the account and pauses its sessions.
        supervision::failed(pool, account, "monitor_stopped").await
    } else {
        observer::pause(pool, account, id).await
    }
}

fn load_signer(account: &str) -> Result<TestnetSigner> {
    let path = std::env::var("TESTNET_PRIVATE_KEY_FILE")
        .map_err(|_| anyhow::anyhow!("configure TESTNET_PRIVATE_KEY_FILE"))?;
    let metadata =
        std::fs::metadata(&path).map_err(|_| anyhow::anyhow!("testnet key file unavailable"))?;
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
    Ok(signer)
}
