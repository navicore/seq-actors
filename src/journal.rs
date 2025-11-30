//! Event journal for actor persistence
//!
//! The journal stores events as append-only binary logs, enabling:
//! - Actor state recovery after restart
//! - Event replay for debugging
//! - Audit trail of all state changes
//!
//! # Storage Format
//!
//! Events are stored as length-prefixed bincode records:
//! ```text
//! [4 bytes: length][bincode event data]
//! [4 bytes: length][bincode event data]
//! ...
//! ```
//!
//! This format is:
//! - Fast to read/write (no parsing overhead)
//! - Compact (binary encoding)
//! - Streamable (can read events one at a time)
//!
//! # Debugging
//!
//! Use `Event::to_debug_string()` or the journal inspection utilities
//! for human-readable output when debugging.

use crate::actor::ActorId;
use crate::serialize::TypedValue;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

/// A persisted event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Sequence number within this actor's journal
    pub seq: u64,

    /// Event type (e.g., "Deposit", "Withdraw")
    pub event_type: String,

    /// Event payload (the actual data)
    pub payload: TypedValue,

    /// Unix timestamp (milliseconds)
    pub ts: u64,
}

impl Event {
    pub fn new(seq: u64, event_type: String, payload: TypedValue) -> Self {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Event {
            seq,
            event_type,
            payload,
            ts,
        }
    }

    /// Serialize to binary format
    pub fn to_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Deserialize from binary format
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        bincode::deserialize(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Human-readable debug representation
    pub fn to_debug_string(&self) -> String {
        format!(
            "[seq={}, ts={}, type={}] {}",
            self.seq,
            self.ts,
            self.event_type,
            self.payload.to_debug_string()
        )
    }
}

/// A snapshot of actor state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Sequence number this snapshot was taken at
    pub seq: u64,

    /// The actor's state at this point
    pub state: TypedValue,

    /// Unix timestamp (milliseconds)
    pub ts: u64,
}

impl Snapshot {
    /// Serialize to binary format
    pub fn to_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Deserialize from binary format
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        bincode::deserialize(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// File-based event journal
///
/// Stores events in `{base_path}/{actor_id}/journal.bin`
pub struct Journal {
    base_path: PathBuf,
}

impl Journal {
    /// Create a new journal with the given base path
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Journal {
            base_path: base_path.into(),
        }
    }

    /// Get the journal directory for an actor
    fn actor_dir(&self, actor_id: &ActorId) -> PathBuf {
        self.base_path.join(actor_id.as_str())
    }

    /// Get the journal file path for an actor
    fn journal_path(&self, actor_id: &ActorId) -> PathBuf {
        self.actor_dir(actor_id).join("journal.bin")
    }

    /// Get the snapshot file path for an actor
    fn snapshot_path(&self, actor_id: &ActorId) -> PathBuf {
        self.actor_dir(actor_id).join("snapshot.bin")
    }

    /// Ensure the actor's journal directory exists
    fn ensure_dir(&self, actor_id: &ActorId) -> std::io::Result<()> {
        fs::create_dir_all(self.actor_dir(actor_id))
    }

    /// Append an event to the journal
    ///
    /// Format: [4-byte length][bincode data]
    pub fn append(&self, actor_id: &ActorId, event: &Event) -> std::io::Result<()> {
        self.ensure_dir(actor_id)?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.journal_path(actor_id))?;

        let data = event.to_bytes()?;
        let len = data.len() as u32;

        // Write length prefix (little-endian)
        file.write_all(&len.to_le_bytes())?;
        // Write event data
        file.write_all(&data)?;

        Ok(())
    }

    /// Read all events for an actor
    pub fn read_events(&self, actor_id: &ActorId) -> std::io::Result<Vec<Event>> {
        let path = self.journal_path(actor_id);

        if !path.exists() {
            return Ok(vec![]);
        }

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut events = vec![];
        let mut len_buf = [0u8; 4];

        loop {
            // Read length prefix
            match reader.read_exact(&mut len_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let len = u32::from_le_bytes(len_buf) as usize;

            // Read event data
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data)?;

            let event = Event::from_bytes(&data)?;
            events.push(event);
        }

        Ok(events)
    }

    /// Read events after a specific sequence number
    pub fn read_events_after(&self, actor_id: &ActorId, after_seq: u64) -> std::io::Result<Vec<Event>> {
        let events = self.read_events(actor_id)?;
        Ok(events.into_iter().filter(|e| e.seq > after_seq).collect())
    }

    /// Save a snapshot
    pub fn save_snapshot(&self, actor_id: &ActorId, snapshot: &Snapshot) -> std::io::Result<()> {
        self.ensure_dir(actor_id)?;

        let data = snapshot.to_bytes()?;
        let file = File::create(self.snapshot_path(actor_id))?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&data)?;

        Ok(())
    }

    /// Load the latest snapshot
    pub fn load_snapshot(&self, actor_id: &ActorId) -> std::io::Result<Option<Snapshot>> {
        let path = self.snapshot_path(actor_id);

        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read(path)?;
        let snapshot = Snapshot::from_bytes(&data)?;

        Ok(Some(snapshot))
    }

    /// Check if an actor has any persisted state
    pub fn exists(&self, actor_id: &ActorId) -> bool {
        self.actor_dir(actor_id).exists()
    }

    /// Dump journal contents as debug strings (for inspection)
    pub fn dump_debug(&self, actor_id: &ActorId) -> std::io::Result<Vec<String>> {
        let events = self.read_events(actor_id)?;
        Ok(events.iter().map(|e| e.to_debug_string()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialize::MapKey;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    #[test]
    fn test_append_and_read_events() {
        let temp_dir = TempDir::new().unwrap();
        let journal = Journal::new(temp_dir.path());

        let actor_id = ActorId::new();

        // Append events
        let mut payload1 = BTreeMap::new();
        payload1.insert(MapKey::String("amount".to_string()), TypedValue::Int(100));
        let event1 = Event::new(0, "Deposit".to_string(), TypedValue::Map(payload1));

        let mut payload2 = BTreeMap::new();
        payload2.insert(MapKey::String("amount".to_string()), TypedValue::Int(50));
        let event2 = Event::new(1, "Withdraw".to_string(), TypedValue::Map(payload2));

        journal.append(&actor_id, &event1).unwrap();
        journal.append(&actor_id, &event2).unwrap();

        // Read events
        let events = journal.read_events(&actor_id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "Deposit");
        assert_eq!(events[1].event_type, "Withdraw");
    }

    #[test]
    fn test_read_events_after() {
        let temp_dir = TempDir::new().unwrap();
        let journal = Journal::new(temp_dir.path());

        let actor_id = ActorId::new();

        for i in 0..5 {
            let event = Event::new(i, format!("Event{}", i), TypedValue::Int(i as i64));
            journal.append(&actor_id, &event).unwrap();
        }

        let events = journal.read_events_after(&actor_id, 2).unwrap();
        assert_eq!(events.len(), 2); // seq 3 and 4
        assert_eq!(events[0].seq, 3);
        assert_eq!(events[1].seq, 4);
    }

    #[test]
    fn test_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let journal = Journal::new(temp_dir.path());

        let actor_id = ActorId::new();

        let mut state = BTreeMap::new();
        state.insert(MapKey::String("balance".to_string()), TypedValue::Int(500));

        let snapshot = Snapshot {
            seq: 10,
            state: TypedValue::Map(state),
            ts: 1234567890,
        };

        journal.save_snapshot(&actor_id, &snapshot).unwrap();

        let loaded = journal.load_snapshot(&actor_id).unwrap().unwrap();
        assert_eq!(loaded.seq, 10);
        if let TypedValue::Map(m) = &loaded.state {
            assert_eq!(m.get(&MapKey::String("balance".to_string())), Some(&TypedValue::Int(500)));
        } else {
            panic!("Expected Map");
        }
    }

    #[test]
    fn test_nonexistent_actor() {
        let temp_dir = TempDir::new().unwrap();
        let journal = Journal::new(temp_dir.path());

        let actor_id = ActorId::new();

        assert!(!journal.exists(&actor_id));
        assert_eq!(journal.read_events(&actor_id).unwrap().len(), 0);
        assert!(journal.load_snapshot(&actor_id).unwrap().is_none());
    }

    #[test]
    fn test_debug_dump() {
        let temp_dir = TempDir::new().unwrap();
        let journal = Journal::new(temp_dir.path());

        let actor_id = ActorId::new();

        let event = Event::new(0, "Test".to_string(), TypedValue::String("data".to_string()));
        journal.append(&actor_id, &event).unwrap();

        let debug = journal.dump_debug(&actor_id).unwrap();
        assert_eq!(debug.len(), 1);
        assert!(debug[0].contains("Test"));
        assert!(debug[0].contains("\"data\""));
    }
}
