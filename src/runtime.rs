//! Actor Runtime - Erlang-style actors on May coroutines
//!
//! This module provides the actor runtime that bridges seq-actors with
//! the `may` coroutine infrastructure in seq-runtime.
//!
//! # Architecture
//!
//! Each actor is a `may` coroutine with:
//! - A mailbox (channel) for receiving messages
//! - Persistent state (Map)
//! - A behavior (Seq quotation reference)
//! - A journal for event sourcing
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │              Actor Runtime                   │
//! │  ┌─────────┐  ┌─────────┐  ┌─────────┐     │
//! │  │ Actor 1 │  │ Actor 2 │  │ Actor 3 │     │
//! │  │ ┌─────┐ │  │ ┌─────┐ │  │ ┌─────┐ │     │
//! │  │ │Mail │ │  │ │Mail │ │  │ │Mail │ │     │
//! │  │ │ box │ │  │ │ box │ │  │ │ box │ │     │
//! │  │ └─────┘ │  │ └─────┘ │  │ └─────┘ │     │
//! │  │  State  │  │  State  │  │  State  │     │
//! │  │ Journal │  │ Journal │  │ Journal │     │
//! │  └─────────┘  └─────────┘  └─────────┘     │
//! │                                             │
//! │         May Coroutine Scheduler             │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # Message Flow
//!
//! 1. `actor-send` looks up actor's mailbox channel
//! 2. Message sent to channel (cooperative yield if full)
//! 3. Actor coroutine receives message (cooperative yield if empty)
//! 4. Event journaled before state mutation
//! 5. Behavior quotation executed: (State, Msg) → State'
//! 6. State updated, loop continues

use crate::actor::ActorId;
use crate::journal::{Event, Journal, Snapshot};
use crate::serialize::TypedValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

/// Actor mailbox - wraps a channel ID for type safety
#[derive(Debug, Clone, Copy)]
pub struct Mailbox {
    /// Channel ID from seq-runtime
    channel_id: i64,
}

impl Mailbox {
    pub fn new(channel_id: i64) -> Self {
        Mailbox { channel_id }
    }

    pub fn channel_id(&self) -> i64 {
        self.channel_id
    }
}

/// Actor entry in the registry
#[derive(Debug)]
struct ActorEntry {
    /// Mailbox for sending messages to this actor
    mailbox: Mailbox,
    /// Behavior name (quotation to execute)
    /// Used when dispatching messages to run the actor's behavior
    #[allow(dead_code)]
    behavior: String,
    /// Whether actor is running
    running: bool,
}

/// Global actor registry
///
/// Maps ActorId → ActorEntry (mailbox, behavior, status)
/// Thread-safe for access from multiple coroutines.
pub(crate) struct ActorRegistry {
    actors: RwLock<HashMap<ActorId, ActorEntry>>,
}

impl ActorRegistry {
    fn new() -> Self {
        ActorRegistry {
            actors: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new actor
    pub(crate) fn register(&self, id: ActorId, mailbox: Mailbox, behavior: String) {
        let mut actors = self.actors.write().expect("registry write lock poisoned");
        actors.insert(
            id,
            ActorEntry {
                mailbox,
                behavior,
                running: true,
            },
        );
    }

    /// Get mailbox for an actor
    fn get_mailbox(&self, id: &ActorId) -> Option<Mailbox> {
        let actors = self.actors.read().expect("registry read lock poisoned");
        actors.get(id).map(|e| e.mailbox)
    }

    /// Mark actor as stopped
    fn mark_stopped(&self, id: &ActorId) {
        let mut actors = self.actors.write().expect("registry write lock poisoned");
        if let Some(entry) = actors.get_mut(id) {
            entry.running = false;
        }
    }

    /// Remove actor from registry
    fn unregister(&self, id: &ActorId) {
        let mut actors = self.actors.write().expect("registry write lock poisoned");
        actors.remove(id);
    }

    /// Check if actor exists and is running
    fn is_running(&self, id: &ActorId) -> bool {
        let actors = self.actors.read().expect("registry read lock poisoned");
        actors.get(id).is_some_and(|e| e.running)
    }
}

// Global registry instance (pub(crate) for FFI access)
lazy_static::lazy_static! {
    pub(crate) static ref REGISTRY: ActorRegistry = ActorRegistry::new();
}

/// Actor runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Base path for journal storage
    pub journal_path: PathBuf,
    /// Whether to enable journaling
    pub journaling_enabled: bool,
    /// Snapshot interval (events between snapshots)
    pub snapshot_interval: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            journal_path: PathBuf::from("./actors"),
            journaling_enabled: true,
            snapshot_interval: 100,
        }
    }
}

/// Actor runtime state
///
/// Manages the lifecycle of all actors in the system.
pub struct ActorRuntime {
    config: RuntimeConfig,
    journal: Journal,
}

impl ActorRuntime {
    /// Create a new actor runtime
    pub fn new(config: RuntimeConfig) -> Self {
        let journal = Journal::new(&config.journal_path);
        ActorRuntime { config, journal }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(RuntimeConfig::default())
    }

    /// Get reference to journal
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Register an actor (called after coroutine spawned)
    pub fn register_actor(&self, id: ActorId, mailbox: Mailbox, behavior: String) {
        REGISTRY.register(id, mailbox, behavior);
    }

    /// Get mailbox for sending to an actor
    pub fn get_mailbox(&self, id: &ActorId) -> Option<Mailbox> {
        REGISTRY.get_mailbox(id)
    }

    /// Check if actor is running
    pub fn is_running(&self, id: &ActorId) -> bool {
        REGISTRY.is_running(id)
    }

    /// Mark actor as stopped
    pub fn stop_actor(&self, id: &ActorId) {
        REGISTRY.mark_stopped(id);
    }

    /// Unregister actor (cleanup)
    pub fn unregister_actor(&self, id: &ActorId) {
        REGISTRY.unregister(id);
    }

    /// Recover actor state from journal
    ///
    /// Returns (state, sequence_number) or None if no persisted state
    pub fn recover_state(&self, id: &ActorId) -> std::io::Result<Option<(TypedValue, u64)>> {
        // Try to load snapshot first
        if let Some(snapshot) = self.journal.load_snapshot(id)? {
            // Replay events after snapshot
            let events = self.journal.read_events_after(id, snapshot.seq)?;

            if events.is_empty() {
                return Ok(Some((snapshot.state, snapshot.seq)));
            }

            // TODO: Replay events to rebuild state
            // For now, just return snapshot state
            let final_seq = events.last().map(|e| e.seq).unwrap_or(snapshot.seq);
            Ok(Some((snapshot.state, final_seq)))
        } else {
            // No snapshot, replay all events
            let events = self.journal.read_events(id)?;

            if events.is_empty() {
                return Ok(None);
            }

            // TODO: Replay events to rebuild state
            // For now, return empty map
            let final_seq = events.last().map(|e| e.seq).unwrap_or(0);
            Ok(Some((TypedValue::Map(std::collections::BTreeMap::new()), final_seq)))
        }
    }

    /// Persist an event to the journal
    pub fn persist_event(&self, id: &ActorId, event: &Event) -> std::io::Result<()> {
        if self.config.journaling_enabled {
            self.journal.append(id, event)?;
        }
        Ok(())
    }

    /// Save a snapshot
    pub fn save_snapshot(&self, id: &ActorId, state: &TypedValue, seq: u64) -> std::io::Result<()> {
        if self.config.journaling_enabled {
            let snapshot = Snapshot {
                seq,
                state: state.clone(),
                ts: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
            };
            self.journal.save_snapshot(id, &snapshot)?;
        }
        Ok(())
    }
}

// Thread-local storage for current actor context
thread_local! {
    static CURRENT_ACTOR_ID: std::cell::RefCell<Option<ActorId>> = const { std::cell::RefCell::new(None) };
}

/// Set the current actor ID (called when entering actor coroutine)
pub fn set_current_actor(id: ActorId) {
    CURRENT_ACTOR_ID.with(|cell| {
        *cell.borrow_mut() = Some(id);
    });
}

/// Get the current actor ID (for actor-self builtin)
pub fn get_current_actor() -> Option<ActorId> {
    CURRENT_ACTOR_ID.with(|cell| cell.borrow().clone())
}

/// Clear the current actor ID (called when exiting actor coroutine)
pub fn clear_current_actor() {
    CURRENT_ACTOR_ID.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_operations() {
        let id = ActorId::new();
        let mailbox = Mailbox::new(42);

        REGISTRY.register(id.clone(), mailbox, "test-behavior".to_string());

        assert!(REGISTRY.is_running(&id));
        assert_eq!(REGISTRY.get_mailbox(&id).unwrap().channel_id(), 42);

        REGISTRY.mark_stopped(&id);
        assert!(!REGISTRY.is_running(&id));

        REGISTRY.unregister(&id);
        assert!(REGISTRY.get_mailbox(&id).is_none());
    }

    #[test]
    fn test_runtime_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RuntimeConfig {
            journal_path: temp_dir.path().to_path_buf(),
            journaling_enabled: true,
            snapshot_interval: 100,
        };

        let runtime = ActorRuntime::new(config);
        assert!(runtime.config.journaling_enabled);
    }

    #[test]
    fn test_current_actor_thread_local() {
        let id = ActorId::new();

        assert!(get_current_actor().is_none());

        set_current_actor(id.clone());
        assert_eq!(get_current_actor().unwrap(), id);

        clear_current_actor();
        assert!(get_current_actor().is_none());
    }

    #[test]
    fn test_recover_empty_state() {
        let temp_dir = TempDir::new().unwrap();
        let config = RuntimeConfig {
            journal_path: temp_dir.path().to_path_buf(),
            journaling_enabled: true,
            snapshot_interval: 100,
        };

        let runtime = ActorRuntime::new(config);
        let id = ActorId::new();

        // No persisted state for new actor
        let result = runtime.recover_state(&id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_persist_and_recover() {
        let temp_dir = TempDir::new().unwrap();
        let config = RuntimeConfig {
            journal_path: temp_dir.path().to_path_buf(),
            journaling_enabled: true,
            snapshot_interval: 100,
        };

        let runtime = ActorRuntime::new(config);
        let id = ActorId::new();

        // Persist some events
        let event = Event::new(0, "TestEvent".to_string(), TypedValue::Int(42));
        runtime.persist_event(&id, &event).unwrap();

        // Recover should find something
        let result = runtime.recover_state(&id).unwrap();
        assert!(result.is_some());
    }
}
