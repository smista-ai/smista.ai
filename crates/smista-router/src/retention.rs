//! This module exposes the [`RetentionService`] struct, which is responsible for managing the retention and cleanup of the
//! data stored by the database.

use std::time::Duration;

use smista_storage::database::Database;
use smista_storage::database::surreal::SurrealDatabase;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Configuration for configuring the [`RetentionService`].
pub struct RetentionServiceConfig {
    /// The database instance to perform cleanup on.
    pub database: SurrealDatabase,
    /// Cancellation token to signal the service to stop.
    pub exit: CancellationToken,
    /// Days to retain traces.
    pub trace_retention_days: u32,
    /// Days to retain sessions.
    pub session_retention_days: u32,
    /// Days to retain archived sessions before purge.
    pub archived_session_retention_days: u32,
    /// Cleanup interval.
    pub cleanup_interval: Duration,
}

/// The `RetentionService` struct is responsible for managing the retention and cleanup of the data stored by the database.
pub struct RetentionService {
    /// The database instance to perform cleanup on.
    database: SurrealDatabase,
    /// Cancellation token to signal the service to stop.
    exit: CancellationToken,
    /// Days to retain traces.
    trace_retention_days: u32,
    /// Days to retain sessions.
    session_retention_days: u32,
    /// Days to retain archived sessions before purge.
    archived_session_retention_days: u32,
    /// Cleanup interval.
    cleanup_interval: Duration,
}

impl RetentionService {
    /// Creates a new instance of the [`RetentionService`] with the given configuration.
    pub fn new(config: RetentionServiceConfig) -> Self {
        Self {
            database: config.database,
            exit: config.exit,
            trace_retention_days: config.trace_retention_days,
            session_retention_days: config.session_retention_days,
            archived_session_retention_days: config.archived_session_retention_days,
            cleanup_interval: config.cleanup_interval,
        }
    }

    /// Runs the retention service in a background task.
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(self.do_run())
    }

    /// Main loop of the retention service, which performs cleanup at regular intervals until the `exit` token is triggered.
    async fn do_run(self) {
        tracing::debug!("RetentionService started");

        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now() + self.cleanup_interval,
            self.cleanup_interval,
        );
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    for err in self.cleanup().await {
                        tracing::error!("Error during retention cleanup: {err}");
                    }
                }
                _ = self.exit.cancelled() => {
                    tracing::info!("Retention service received exit signal, shutting down...");
                    break;
                }
            }
        }

        tracing::debug!("RetentionService stopped");
    }

    /// Performs the actual cleanup of expired and old data from the database.
    ///
    /// - Deletes expired auth tokens.
    /// - Deletes expired sessions.
    /// - Deletes archived sessions that have exceeded the retention period.
    /// - Deletes old traces that have exceeded the retention period.
    ///
    /// Returns a vector of any errors that occurred during the cleanup process.
    async fn cleanup(&self) -> Vec<anyhow::Error> {
        let mut errors = Vec::with_capacity(4);
        tracing::debug!("Starting retention cleanup");

        tracing::debug!("Cleaning up expired and old auth tokens...");
        if let Err(err) = self.database.delete_expired_tokens().await {
            errors.push(anyhow::anyhow!("Failed to delete expired tokens: {err}"));
        }

        tracing::debug!("Cleaning up expired sessions...");
        if let Err(err) = self
            .database
            .purge_old_sessions(self.session_retention_days)
            .await
        {
            errors.push(anyhow::anyhow!("Failed to delete expired sessions: {err}"));
        }

        tracing::debug!("Cleaning up archived sessions...");
        if let Err(err) = self
            .database
            .purge_archived_sessions(self.archived_session_retention_days)
            .await
        {
            errors.push(anyhow::anyhow!("Failed to delete archived sessions: {err}"));
        }

        tracing::debug!("Cleaning up old traces...");
        if let Err(err) = self.database.purge_traces(self.trace_retention_days).await {
            errors.push(anyhow::anyhow!("Failed to delete old traces: {err}"));
        }

        tracing::debug!("Retention cleanup completed with {} error(s)", errors.len());

        errors
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Duration as ChronoDuration, Utc};
    use smista_storage::database::surreal::{SurrealBackend, SurrealOptions};
    use smista_storage::entity::{Session, Table, User};
    use surrealdb::types::RecordId;
    use uuid::Uuid;

    use super::*;

    /// The session retention window used by the test service: a year.
    const SESSION_RETENTION_DAYS: u32 = 365;
    /// The archived-session retention window used by the test service: a month.
    const ARCHIVED_RETENTION_DAYS: u32 = 30;

    async fn memory_db() -> SurrealDatabase {
        SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database")
    }

    fn user(id: Uuid) -> User {
        User {
            id: RecordId::new(User::name(), id.to_string()),
            api_key_hash: format!("hash-{id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        }
    }

    fn session(
        id: Uuid,
        user_id: Uuid,
        updated_at: DateTime<Utc>,
        archived_at: Option<DateTime<Utc>>,
    ) -> Session {
        Session {
            id: RecordId::new(Session::name(), id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: Some("session".to_string()),
            created_at: Utc::now(),
            updated_at,
            archived_at,
        }
    }

    fn service(database: SurrealDatabase, exit: CancellationToken) -> RetentionService {
        RetentionService::new(RetentionServiceConfig {
            database,
            exit,
            trace_retention_days: 90,
            session_retention_days: SESSION_RETENTION_DAYS,
            archived_session_retention_days: ARCHIVED_RETENTION_DAYS,
            // Long enough that the periodic tick never fires during a test; the
            // cleanup path is driven directly and the loop only exercises exit.
            cleanup_interval: Duration::from_secs(3_600),
        })
    }

    #[tokio::test]
    async fn should_report_no_errors_when_cleaning_empty_database() {
        let svc = service(memory_db().await, CancellationToken::new());

        let errors = svc.cleanup().await;

        assert!(errors.is_empty(), "unexpected cleanup errors: {errors:?}");
    }

    #[tokio::test]
    async fn should_purge_sessions_past_their_retention_window() {
        let db = memory_db().await;

        let user_id = Uuid::now_v7();
        db.create_user(user(user_id))
            .await
            .expect("failed to create user");

        // Older than the retention window, so the purge removes it.
        let old_id = Uuid::now_v7();
        db.create_session(session(
            old_id,
            user_id,
            Utc::now() - ChronoDuration::days(i64::from(SESSION_RETENTION_DAYS) + 1),
            None,
        ))
        .await
        .expect("failed to create old session");

        // Within the window, so it must survive.
        let recent_id = Uuid::now_v7();
        db.create_session(session(recent_id, user_id, Utc::now(), None))
            .await
            .expect("failed to create recent session");

        let errors = service(db.clone(), CancellationToken::new())
            .cleanup()
            .await;
        assert!(errors.is_empty(), "unexpected cleanup errors: {errors:?}");

        assert!(
            db.get_session(user_id, old_id)
                .await
                .expect("failed to load old session")
                .is_none(),
            "session past its retention window was not purged"
        );
        assert!(
            db.get_session(user_id, recent_id)
                .await
                .expect("failed to load recent session")
                .is_some(),
            "session within its retention window was purged"
        );
    }

    #[tokio::test]
    async fn should_purge_archived_sessions_past_their_retention_window() {
        let db = memory_db().await;

        let user_id = Uuid::now_v7();
        db.create_user(user(user_id))
            .await
            .expect("failed to create user");

        // Archived longer ago than the archived window: purged. Its `updated_at`
        // is recent, proving the archived window — not the session window — governs.
        let old_id = Uuid::now_v7();
        db.create_session(session(
            old_id,
            user_id,
            Utc::now(),
            Some(Utc::now() - ChronoDuration::days(i64::from(ARCHIVED_RETENTION_DAYS) + 1)),
        ))
        .await
        .expect("failed to create old archived session");

        // Archived within the window: retained.
        let recent_id = Uuid::now_v7();
        db.create_session(session(recent_id, user_id, Utc::now(), Some(Utc::now())))
            .await
            .expect("failed to create recent archived session");

        let errors = service(db.clone(), CancellationToken::new())
            .cleanup()
            .await;
        assert!(errors.is_empty(), "unexpected cleanup errors: {errors:?}");

        assert!(
            db.get_session(user_id, old_id)
                .await
                .expect("failed to load old archived session")
                .is_none(),
            "archived session past its retention window was not purged"
        );
        assert!(
            db.get_session(user_id, recent_id)
                .await
                .expect("failed to load recent archived session")
                .is_some(),
            "archived session within its retention window was purged"
        );
    }

    #[tokio::test]
    async fn should_stop_run_loop_when_exit_token_is_cancelled() {
        let exit = CancellationToken::new();
        let handle = service(memory_db().await, exit.clone()).run();

        // Cancelling resolves the select's exit branch before the first tick.
        exit.cancel();

        handle.await.expect("retention task panicked");
    }
}
