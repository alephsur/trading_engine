use crate::{
    model::{self, Intent},
    signing::TestnetSigner,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use trading_domain::data::now_ms;
use trading_paper::dec;

pub async fn event(
    tx: &mut Transaction<'_, Postgres>,
    account: &str,
    id: &str,
    kind: &str,
    payload: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO testnet_events(account,command_id,observed_ms,kind,payload) VALUES ($1,$2,$3,$4,$5)").bind(account).bind(id).bind(now_ms()).bind(kind).bind(payload).execute(&mut **tx).await?;
    Ok(())
}
pub async fn existing(pool: &PgPool, account: &str, id: &str, intent: &Intent) -> Result<bool> {
    let previous: Option<Value> = sqlx::query_scalar(
        "SELECT intent FROM testnet_requests WHERE account=$1 AND command_id=$2",
    )
    .bind(account)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some(previous) = previous {
        ensure!(
            previous == serde_json::to_value(intent)?,
            "command id reused with different intent"
        );
        return Ok(true);
    }
    Ok(false)
}
/// Persist before any network write. Account and signer locks serialize preparation.
pub async fn prepare(
    pool: &PgPool,
    account: &str,
    id: &str,
    intent: &Intent,
    action: Value,
    signer: &TestnetSigner,
    state: &Value,
) -> Result<bool> {
    model::command_id(id)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,701))")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    let previous: Option<Value> = sqlx::query_scalar(
        "SELECT intent FROM testnet_requests WHERE account=$1 AND command_id=$2",
    )
    .bind(account)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(previous) = previous {
        ensure!(
            previous == serde_json::to_value(intent)?,
            "command id reused with different intent"
        );
        tx.commit().await?;
        return Ok(false);
    }
    let now = now_ms();
    let equity = model::decimal(&state["marginSummary"]["accountValue"])?;
    if matches!(intent, Intent::Entry { .. }) {
        ensure!(
            monitor_allows(&mut tx, account, now).await?,
            "testnet supervisor blocks new entries"
        );
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM testnet_requests WHERE account=$1 AND state NOT IN ('rejected','expired') AND COALESCE(reconciliation->>'safe_for_entry','false') <> 'true')").bind(account).fetch_one(&mut *tx).await?;
        ensure!(
            !blocked,
            "previous requests require reconciliation before another entry"
        );
        sqlx::query("INSERT INTO testnet_days(account,day,initial_equity) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(account).bind(now/86_400_000).bind(equity.to_string()).execute(&mut *tx).await?;
        let (initial,latched,attempts):(String,bool,i32)=sqlx::query_as("SELECT initial_equity,loss_latched,entry_attempts FROM testnet_days WHERE account=$1 AND day=$2 FOR UPDATE").bind(account).bind(now/86_400_000).fetch_one(&mut *tx).await?;
        let loss = equity <= initial.parse::<rust_decimal::Decimal>()? * dec("0.99");
        if loss {
            sqlx::query("UPDATE testnet_days SET loss_latched=true WHERE account=$1 AND day=$2")
                .bind(account)
                .bind(now / 86_400_000)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            anyhow::bail!("testnet daily loss limit reached")
        }
        ensure!(
            !latched && attempts < 10,
            "testnet daily entry limit or loss latch active"
        );
        sqlx::query(
            "UPDATE testnet_days SET entry_attempts=entry_attempts+1 WHERE account=$1 AND day=$2",
        )
        .bind(account)
        .bind(now / 86_400_000)
        .execute(&mut *tx)
        .await?;
    }
    let address = signer.address();
    sqlx::query(
        "INSERT INTO testnet_signers(signer,account,nonce) VALUES ($1,$2,0) ON CONFLICT DO NOTHING",
    )
    .bind(&address)
    .bind(account)
    .execute(&mut *tx)
    .await?;
    let (bound, last): (String, i64) =
        sqlx::query_as("SELECT account,nonce FROM testnet_signers WHERE signer=$1 FOR UPDATE")
            .bind(&address)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(
        bound == account,
        "API wallet already bound to another account"
    );
    let nonce = now.max(
        last.checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("nonce exhausted"))?,
    );
    ensure!(nonce - now < 1000, "nonce clock drift; synchronize clock");
    let expires = nonce + 10_000;
    let signed = signer.sign(action, nonce, expires)?;
    sqlx::query("UPDATE testnet_signers SET nonce=$2 WHERE signer=$1")
        .bind(&address)
        .bind(nonce)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO testnet_requests(account,command_id,signer,nonce,created_ms,expires_ms,intent,signed_request,state) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'prepared')").bind(account).bind(id).bind(&address).bind(nonce).bind(now).bind(expires).bind(serde_json::to_value(intent)?).bind(signed).execute(&mut *tx).await?;
    event(
        &mut tx,
        account,
        id,
        "prepared",
        json!({"intent":intent,"nonce":nonce,"expires_ms":expires}),
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}
/// At-most-once transport attempt. A crash after this commit is intentionally uncertain.
pub async fn claim(pool: &PgPool, account: &str, id: &str) -> Result<Option<Value>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,701))")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    let entry: Option<bool> = sqlx::query_scalar(
        "SELECT intent->>'kind'='entry' FROM testnet_requests WHERE account=$1 AND command_id=$2",
    )
    .bind(account)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    if entry == Some(true) && !monitor_allows(&mut tx, account, now_ms()).await? {
        sqlx::query("UPDATE testnet_requests SET state='expired' WHERE account=$1 AND command_id=$2 AND state='prepared'")
            .bind(account).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(None);
    }
    // Pause/expiry are checked at the durable dispatch boundary, not only before planning.
    // After this transaction commits, an in-flight send may still complete after a pause.
    let guard: Option<(bool,i64,i64)> = sqlx::query_as("SELECT o.paused,o.expires_ms,c.close_ms FROM testnet_auto_claims c JOIN testnet_observer_sessions o USING(account,session_id) WHERE c.account=$1 AND c.command_id=$2 FOR UPDATE OF o")
        .bind(account).bind(id).fetch_optional(&mut *tx).await?;
    if guard.is_some_and(|(paused, expires, close)| {
        paused || now_ms() >= expires || !(1..=120_000).contains(&(now_ms() - close))
    }) {
        sqlx::query("UPDATE testnet_requests SET state='expired' WHERE account=$1 AND command_id=$2 AND state='prepared'")
            .bind(account).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(None);
    }
    sqlx::query("UPDATE testnet_requests SET state='expired' WHERE account=$1 AND command_id=$2 AND state='prepared' AND expires_ms<=$3").bind(account).bind(id).bind(now_ms()).execute(&mut *tx).await?;
    let value:Option<Value>=sqlx::query_scalar("UPDATE testnet_requests SET state='uncertain' WHERE account=$1 AND command_id=$2 AND state='prepared' AND expires_ms>$3 RETURNING signed_request").bind(account).bind(id).bind(now_ms()).fetch_optional(&mut *tx).await?;
    if value.is_some() {
        event(
            &mut tx,
            account,
            id,
            "dispatch_claimed",
            json!({"outcome":"unknown_until_reconciled"}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(value)
}

async fn monitor_allows(
    tx: &mut Transaction<'_, Postgres>,
    account: &str,
    now: i64,
) -> Result<bool> {
    let state: Option<(i64, bool)> = sqlx::query_as(
        "SELECT observed_ms,blocked FROM testnet_supervision WHERE account=$1 FOR UPDATE",
    )
    .bind(account)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(state.is_none_or(|(observed, blocked)| {
        !blocked && (0..crate::supervision::HEARTBEAT_MS).contains(&(now - observed))
    }))
}
pub async fn record_response(
    pool: &PgPool,
    account: &str,
    id: &str,
    value: Value,
    expected: usize,
) -> Result<()> {
    let state = model::response_state(&value, expected);
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE testnet_requests SET state=$3,response=$4 WHERE account=$1 AND command_id=$2 AND state='uncertain'").bind(account).bind(id).bind(state).bind(&value).execute(&mut *tx).await?;
    event(
        &mut tx,
        account,
        id,
        "exchange_response",
        json!({"classification":state}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
pub async fn report(pool: &PgPool, account: &str) -> Result<Value> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('command_id',command_id,'intent',intent,'state',state,'nonce',nonce,'created_ms',created_ms,'reconciliation',reconciliation,'response',response) FROM testnet_requests WHERE account=$1 ORDER BY created_ms DESC LIMIT 100").bind(account).fetch_all(pool).await?;
    let snapshot: Option<Value> = sqlx::query_scalar(
        "SELECT payload FROM testnet_snapshots WHERE account=$1 ORDER BY id DESC LIMIT 1",
    )
    .bind(account)
    .fetch_optional(pool)
    .await?;
    Ok(
        json!({"network":"TESTNET","account":account,"requests":rows,"latest_snapshot":snapshot,"request_limit":100}),
    )
}
