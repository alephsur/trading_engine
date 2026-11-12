use sqlx::{Connection, PgConnection};
use trading_testnet::runtime::SupervisorLock;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; advisory locks only, no exchange or signing"]
async fn supervisors_exclude_duplicates_release_on_drop_and_fail_on_connection_loss() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = trading_storage::connect(&url).await.unwrap();
    let account = format!(
        "runtime-test-{}-{}",
        std::process::id(),
        trading_domain::data::now_ms()
    );
    let mut owner = SupervisorLock::acquire(&pool, &account).await.unwrap();
    owner.check().await.unwrap();
    assert!(
        SupervisorLock::acquire(&pool, &account.to_uppercase())
            .await
            .is_err()
    );
    let other = SupervisorLock::acquire(&pool, &format!("{account}-other"))
        .await
        .unwrap();
    drop(other);

    // The detached connection closes on drop; no pooled session retains its lock.
    drop(owner);
    let mut owner = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(owner) = SupervisorLock::acquire(&pool, &account).await {
                break owner;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    owner.check().await.unwrap();

    // Identify only this test's dedicated connection by its advisory lock key.
    let mut probe = PgConnection::connect(&url).await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT l.pid FROM pg_locks l WHERE l.locktype='advisory' AND l.database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND l.classid=((hashtextextended($1,704) >> 32) & 4294967295)::oid AND l.objid=(hashtextextended($1,704) & 4294967295)::oid AND l.objsubid=1 AND l.granted")
        .bind(&account).fetch_one(&mut probe).await.unwrap();
    let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .fetch_one(&mut probe)
        .await
        .unwrap();
    assert!(terminated);
    assert!(owner.check().await.is_err());
    assert!(owner.check().await.is_err()); // no implicit reconnection/reacquisition
    let mut replacement = SupervisorLock::acquire(&pool, &account).await.unwrap();
    replacement.check().await.unwrap();
    drop(replacement);
    pool.close().await;
}
