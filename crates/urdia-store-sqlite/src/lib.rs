//! SQLite implementation of the Urdia event store.

use async_trait::async_trait;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};
use urdia_core::{
    Event, EventKind, EventStore, Result, SessionId, StoredEvent, StoredSession, UrdiaError,
};
use uuid::Uuid;

/// An append-only SQLite event store.
#[derive(Clone)]
pub struct SqliteEventStore {
    connection: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for SqliteEventStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SqliteEventStore { connection: <redacted> }")
    }
}

impl SqliteEventStore {
    /// Open or create a database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path).map_err(store_error)?;
        Self::from_connection(connection)
    }

    /// Open an existing event database without write or create access.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(store_error)?;
        let has_events_table = connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'events'",
                [],
                |_| Ok(()),
            )
            .optional()
            .map_err(store_error)?
            .is_some();
        if !has_events_table {
            return Err(UrdiaError::Store(
                "event database does not contain the events table".to_owned(),
            ));
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Create an in-memory store for tests and ephemeral sessions.
    pub fn in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory().map_err(store_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    schema_version INTEGER NOT NULL,
                    id TEXT NOT NULL UNIQUE,
                    session_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    occurred_at_ms INTEGER NOT NULL,
                    data TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS events_session_idx ON events(session_id, sequence);
                PRAGMA user_version = 1;",
            )
            .map_err(store_error)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
}

#[async_trait]
impl EventStore for SqliteEventStore {
    async fn append(&self, event: Event) -> Result<()> {
        let kind = serde_json::to_string(&event.kind).map_err(store_error)?;
        let data = serde_json::to_string(&event.data).map_err(store_error)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| UrdiaError::Store("event store lock poisoned".to_owned()))?;
        connection
            .execute(
                "INSERT INTO events
                 (schema_version, id, session_id, kind, occurred_at_ms, data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event.schema_version,
                    event.id.to_string(),
                    event.session_id.to_string(),
                    kind,
                    event.occurred_at_ms,
                    data
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }

    async fn list(&self, session_id: &SessionId) -> Result<Vec<StoredEvent>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| UrdiaError::Store("event store lock poisoned".to_owned()))?;
        let mut statement = connection
            .prepare(
                "SELECT sequence, schema_version, id, session_id, kind, occurred_at_ms, data
                 FROM events WHERE session_id = ?1 ORDER BY sequence ASC",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(params![session_id.to_string()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, u16>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(store_error)?;
        let mut events = Vec::new();
        for row in rows {
            let (sequence, schema_version, id, stored_session, kind, occurred_at_ms, data) =
                row.map_err(store_error)?;
            let event_id = Uuid::parse_str(&id).map_err(store_error)?;
            let stored_session_id: SessionId =
                serde_json::from_str(&format!("\"{stored_session}\"")).map_err(store_error)?;
            let event_kind = serde_json::from_str::<EventKind>(&kind).map_err(store_error)?;
            let event_data = serde_json::from_str::<Value>(&data).map_err(store_error)?;
            events.push(StoredEvent {
                sequence,
                event: Event {
                    schema_version,
                    id: event_id,
                    session_id: stored_session_id,
                    kind: event_kind,
                    occurred_at_ms,
                    data: event_data,
                },
            });
        }
        Ok(events)
    }

    async fn list_sessions(&self, limit: usize) -> Result<Vec<StoredSession>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let session_started_kind =
            serde_json::to_string(&EventKind::SessionStarted).map_err(store_error)?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let connection = self
            .connection
            .lock()
            .map_err(|_| UrdiaError::Store("event store lock poisoned".to_owned()))?;
        let mut statement = connection
            .prepare(
                "SELECT e.session_id,
                        MIN(e.sequence),
                        MAX(e.sequence),
                        COUNT(*),
                        (SELECT first.occurred_at_ms
                         FROM events AS first
                         WHERE first.session_id = e.session_id
                         ORDER BY first.sequence ASC
                         LIMIT 1),
                        (SELECT latest.occurred_at_ms
                         FROM events AS latest
                         WHERE latest.session_id = e.session_id
                         ORDER BY latest.sequence DESC
                         LIMIT 1),
                        COALESCE((
                            SELECT started.data
                            FROM events AS started
                            WHERE started.session_id = e.session_id
                              AND started.kind = ?1
                            ORDER BY started.sequence ASC
                            LIMIT 1
                        ), 'null')
                 FROM events AS e
                 GROUP BY e.session_id
                 ORDER BY MAX(e.sequence) DESC
                 LIMIT ?2",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(params![session_started_kind, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(store_error)?;
        let mut sessions = Vec::new();
        for row in rows {
            let (
                stored_session,
                first_sequence,
                last_sequence,
                event_count,
                started_at_ms,
                updated_at_ms,
                session_data,
            ) = row.map_err(store_error)?;
            sessions.push(StoredSession {
                session_id: stored_session.parse().map_err(store_error)?,
                first_sequence,
                last_sequence,
                event_count: u64::try_from(event_count).map_err(store_error)?,
                started_at_ms,
                updated_at_ms,
                session_data: serde_json::from_str(&session_data).map_err(store_error)?,
            });
        }
        Ok(sessions)
    }
}

fn store_error(error: impl std::fmt::Display) -> UrdiaError {
    UrdiaError::Store(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use urdia_core::{Event, EventKind, EventStore, SessionId};

    #[tokio::test]
    async fn appends_and_reads_events_in_order() {
        let store = SqliteEventStore::in_memory().expect("in-memory SQLite should open");
        let session_id = SessionId::new();
        store
            .append(Event::new(
                session_id.clone(),
                EventKind::SessionStarted,
                json!({"provider": "fake"}),
            ))
            .await
            .expect("first event should append");
        store
            .append(Event::new(
                session_id.clone(),
                EventKind::AgentText,
                json!({"text": "hello"}),
            ))
            .await
            .expect("second event should append");

        let events = store.list(&session_id).await.expect("events should read");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
        assert_eq!(events[1].event.kind, EventKind::AgentText);
    }

    #[tokio::test]
    async fn filters_events_by_session() {
        let store = SqliteEventStore::in_memory().expect("in-memory SQLite should open");
        let first = SessionId::new();
        let second = SessionId::new();
        store
            .append(Event::new(first.clone(), EventKind::Error, Value::Null))
            .await
            .expect("event should append");
        store
            .append(Event::new(second.clone(), EventKind::Error, Value::Null))
            .await
            .expect("event should append");
        assert_eq!(store.list(&first).await.expect("first list").len(), 1);
        assert_eq!(store.list(&second).await.expect("second list").len(), 1);
    }

    #[tokio::test]
    async fn persists_events_across_reopen() {
        let directory = tempfile::tempdir().expect("temporary directory should open");
        let path = directory.path().join("events.sqlite");
        let session_id = SessionId::new();
        {
            let store = SqliteEventStore::open(&path).expect("SQLite file should open");
            store
                .append(Event::new(
                    session_id.clone(),
                    EventKind::SessionStarted,
                    json!({"provider": "fake"}),
                ))
                .await
                .expect("event should append");
        }

        let reopened = SqliteEventStore::open(path).expect("SQLite file should reopen");
        let events = reopened
            .list(&session_id)
            .await
            .expect("persisted event should read");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.kind, EventKind::SessionStarted);
    }

    #[tokio::test]
    async fn lists_recent_sessions_with_started_metadata() {
        let store = SqliteEventStore::in_memory().expect("in-memory SQLite should open");
        let first = SessionId::new();
        let second = SessionId::new();
        store
            .append(Event::new(
                first.clone(),
                EventKind::SessionStarted,
                json!({"provider": "qwen"}),
            ))
            .await
            .expect("first session should append");
        store
            .append(Event::new(
                second.clone(),
                EventKind::SessionStarted,
                json!({"provider": "custom"}),
            ))
            .await
            .expect("second session should append");
        store
            .append(Event::new(
                first.clone(),
                EventKind::SessionFinished,
                json!({"status": "completed"}),
            ))
            .await
            .expect("first session should finish");

        let sessions = store.list_sessions(10).await.expect("sessions should list");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, first);
        assert_eq!(sessions[0].event_count, 2);
        assert_eq!(sessions[0].session_data["provider"], "qwen");
        assert_eq!(sessions[1].session_id, second);
        assert!(store.list_sessions(0).await.expect("zero limit").is_empty());
    }

    #[tokio::test]
    async fn read_only_open_never_creates_or_appends() {
        let directory = tempfile::tempdir().expect("temporary directory should open");
        let missing = directory.path().join("missing.sqlite");
        assert!(SqliteEventStore::open_read_only(&missing).is_err());
        assert!(!missing.exists());

        let path = directory.path().join("events.sqlite");
        let session_id = SessionId::new();
        SqliteEventStore::open(&path)
            .expect("database should open")
            .append(Event::new(
                session_id.clone(),
                EventKind::SessionStarted,
                json!({"provider": "qwen"}),
            ))
            .await
            .expect("seed event should append");
        let read_only = SqliteEventStore::open_read_only(path).expect("read-only open");
        assert_eq!(read_only.list(&session_id).await.expect("events").len(), 1);
        assert!(
            read_only
                .append(Event::new(session_id, EventKind::Error, Value::Null))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn session_timestamps_follow_sequence_not_clock_order() {
        let store = SqliteEventStore::in_memory().expect("in-memory SQLite should open");
        let session_id = SessionId::new();
        let mut first = Event::new(
            session_id.clone(),
            EventKind::SessionStarted,
            json!({"provider": "qwen"}),
        );
        first.occurred_at_ms = 2_000;
        let mut second = Event::new(
            session_id.clone(),
            EventKind::SessionFinished,
            json!({"status": "completed"}),
        );
        second.occurred_at_ms = 1_000;
        store.append(first).await.expect("first event");
        store.append(second).await.expect("second event");

        let sessions = store.list_sessions(1).await.expect("sessions");
        assert_eq!(sessions[0].started_at_ms, 2_000);
        assert_eq!(sessions[0].updated_at_ms, 1_000);
    }
}
