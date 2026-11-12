//! Process lifetime controls. They never reset an entry budget or retry a write.
use anyhow::{Result, ensure};
use sqlx::{PgConnection, PgPool};
use tokio::{sync::watch, task::JoinHandle};

/// Keep a dedicated connection: returning a session lock to a pool would leak it.
/// This excludes concurrent CLI supervisors while that connection is alive.
/// Durable claims remain the dispatch fence if a process or connection is lost.
pub struct SupervisorLock {
    connection: PgConnection,
}
impl SupervisorLock {
    pub async fn acquire(pool: &PgPool, account: &str) -> Result<Self> {
        let mut connection = pool.acquire().await?.detach();
        let acquired: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1,704))")
                .bind(account.to_ascii_lowercase())
                .fetch_one(&mut connection)
                .await?;
        ensure!(
            acquired,
            "another TESTNET supervisor already owns this account"
        );
        Ok(Self { connection })
    }

    /// A lost connection must stop this worker; never reconnect and silently
    /// assume that the previous session lock still belongs to this process.
    pub async fn check(&mut self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&mut self.connection)
            .await
            .map_err(|_| {
                anyhow::anyhow!("TESTNET supervisor ownership lost; restart for reconciliation")
            })?;
        Ok(())
    }
}

/// Register before doing work and latch termination even during an HTTP/DB call.
/// The caller finishes its current cycle (without cancelling an in-flight POST),
/// persists a pause/block, and exits before starting another cycle.
pub struct Shutdown {
    received: watch::Receiver<bool>,
    listener: JoinHandle<()>,
}
impl Shutdown {
    pub fn install() -> Result<Self> {
        let (sent, received) = watch::channel(false);
        #[cfg(unix)]
        let listener = {
            use tokio::signal::unix::{SignalKind, signal};
            let mut interrupt = signal(SignalKind::interrupt())?;
            let mut terminate = signal(SignalKind::terminate())?;
            tokio::spawn(async move {
                tokio::select! {
                    _ = interrupt.recv() => {},
                    _ = terminate.recv() => {},
                }
                sent.send_replace(true);
            })
        };
        #[cfg(not(unix))]
        let listener = tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            sent.send_replace(true);
        });
        Ok(Self { received, listener })
    }

    pub fn requested(&self) -> bool {
        *self.received.borrow() || self.received.has_changed().is_err()
    }

    pub async fn wait(&mut self) {
        while !self.requested() {
            if self.received.changed().await.is_err() {
                break;
            }
        }
    }
}
impl Drop for Shutdown {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{process::Command, time::Duration};

    #[test]
    fn termination_during_work_is_latched_without_cancelling_work() {
        const CHILD: &str = "TESTNET_SIGNAL_TEST_READY";
        if let Ok(path) = std::env::var(CHILD) {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut shutdown = Shutdown::install().unwrap();
                std::fs::write(&path, "ready").unwrap();
                // Simulate work which must reach its durable outcome, not be dropped.
                tokio::time::sleep(Duration::from_millis(500)).await;
                std::fs::write(format!("{path}.completed"), "completed").unwrap();
                tokio::time::timeout(Duration::from_secs(5), shutdown.wait())
                    .await
                    .unwrap();
                assert!(shutdown.requested());
                tokio::time::timeout(Duration::from_millis(50), shutdown.wait())
                    .await
                    .unwrap();
            });
            return;
        }
        for signal in ["-TERM", "-INT"] {
            let path = std::env::temp_dir().join(format!(
                "testnet-signal-{}-{}-{signal}",
                std::process::id(),
                trading_domain::data::now_ms()
            ));
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "runtime::tests::termination_during_work_is_latched_without_cancelling_work",
                    "--nocapture",
                ])
                .env(CHILD, &path)
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !path.exists() && child.try_wait().unwrap().is_none() {
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    let _ = child.wait();
                    panic!("signal test child did not become ready");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(path.exists());
            assert!(
                Command::new("kill")
                    .args([signal, &child.id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(child.wait().unwrap().success());
            let completed = format!("{}.completed", path.display());
            assert_eq!(std::fs::read_to_string(&completed).unwrap(), "completed");
            std::fs::remove_file(completed).unwrap();
            std::fs::remove_file(path).unwrap();
        }
    }
}
