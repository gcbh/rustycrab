use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::MemoryConfig;
use crate::storage::MemoryStorage;

/// Background task that flushes Working memory to Episodic based on two rules:
///
/// 1. **Idle timeout**: if no new turns have been retained for
///    `idle_flush_timeout_secs`, flush all Working → Episodic.
/// 2. **Max age backstop**: any Working memory older than
///    `max_working_age_secs` gets force-flushed regardless of activity.
///
/// This runs as a spawned tokio task. Call [`notify_activity`] after each
/// `retain()` to reset the idle timer. Call [`shutdown`] to stop the task.
pub struct SessionFlusher {
    /// Unix timestamp (seconds) of the last retain() call.
    last_activity: Arc<AtomicU64>,
    /// Send `true` to stop the background task.
    shutdown_tx: watch::Sender<bool>,
}

impl SessionFlusher {
    /// Spawn the background flusher task for a given agent.
    ///
    /// Returns a handle that the writer uses to signal activity and
    /// that the system uses to shut down the task.
    pub fn spawn(
        storage: Arc<dyn MemoryStorage>,
        agent_id: Uuid,
        config: &MemoryConfig,
    ) -> Self {
        let last_activity = Arc::new(AtomicU64::new(now_secs()));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let idle_timeout = config.idle_flush_timeout_secs;
        let max_age = config.max_working_age_secs;
        let check_interval = Duration::from_secs(config.flush_check_interval_secs);
        let activity = Arc::clone(&last_activity);

        tokio::spawn(async move {
            run_flush_loop(
                storage,
                agent_id,
                activity,
                shutdown_rx,
                idle_timeout,
                max_age,
                check_interval,
            )
            .await;
        });

        Self {
            last_activity,
            shutdown_tx,
        }
    }

    /// Signal that a new turn was retained. Resets the idle timer.
    pub fn notify_activity(&self) {
        self.last_activity.store(now_secs(), Ordering::Relaxed);
    }

    /// Stop the background flush task.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

impl Drop for SessionFlusher {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn run_flush_loop(
    storage: Arc<dyn MemoryStorage>,
    agent_id: Uuid,
    last_activity: Arc<AtomicU64>,
    mut shutdown_rx: watch::Receiver<bool>,
    idle_timeout_secs: u64,
    max_age_secs: u64,
    check_interval: Duration,
) {
    debug!(
        agent_id = %agent_id,
        idle_timeout = idle_timeout_secs,
        max_age = max_age_secs,
        interval = ?check_interval,
        "session flusher started"
    );

    loop {
        tokio::select! {
            _ = tokio::time::sleep(check_interval) => {},
            Ok(()) = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    // Final flush on shutdown — don't leave Working memories behind.
                    match storage.flush_working_to_episodic(agent_id).await {
                        Ok(n) if n > 0 => {
                            info!(agent_id = %agent_id, flushed = n, "final flush on shutdown");
                        }
                        Err(e) => {
                            warn!(agent_id = %agent_id, error = %e, "final flush failed");
                        }
                        _ => {}
                    }
                    debug!(agent_id = %agent_id, "session flusher stopped");
                    return;
                }
            }
        }

        let now = now_secs();
        let last = last_activity.load(Ordering::Relaxed);
        let idle_secs = now.saturating_sub(last);

        // Rule 1: Idle timeout — flush everything if no activity.
        if idle_secs >= idle_timeout_secs {
            match storage.flush_working_to_episodic(agent_id).await {
                Ok(n) if n > 0 => {
                    info!(
                        agent_id = %agent_id,
                        flushed = n,
                        idle_secs = idle_secs,
                        "idle flush: working → episodic"
                    );
                }
                Err(e) => {
                    warn!(agent_id = %agent_id, error = %e, "idle flush failed");
                }
                _ => {}
            }
            continue;
        }

        // Rule 2: Max age backstop — force-flush old Working memories
        // even during active sessions.
        match storage
            .flush_working_older_than(agent_id, max_age_secs)
            .await
        {
            Ok(n) if n > 0 => {
                info!(
                    agent_id = %agent_id,
                    flushed = n,
                    max_age = max_age_secs,
                    "max-age force flush: working → episodic"
                );
            }
            Err(e) => {
                warn!(agent_id = %agent_id, error = %e, "max-age flush failed");
            }
            _ => {}
        }
    }
}
