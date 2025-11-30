//! Actor types and runtime
//!
//! An actor consists of:
//! - Identity (ActorId)
//! - State (a Seq Map value)
//! - Behavior (a Seq Quotation)
//! - Journal (for event persistence)

use uuid::Uuid;

/// Unique identifier for an actor
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActorId(pub Uuid);

impl ActorId {
    /// Create a new random actor ID
    pub fn new() -> Self {
        ActorId(Uuid::new_v4())
    }

    /// Create from an existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        ActorId(uuid)
    }

    /// Get the UUID as a string
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for ActorId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Reference to an actor (for sending messages)
#[derive(Debug, Clone)]
pub struct ActorRef {
    pub id: ActorId,
    // TODO: Add mailbox sender
}

impl ActorRef {
    pub fn new(id: ActorId) -> Self {
        ActorRef { id }
    }
}

/// Actor instance
///
/// Holds the actor's identity, current state, and behavior.
/// The journal is managed separately to allow different persistence backends.
pub struct Actor {
    /// Unique identifier
    pub id: ActorId,

    /// Current state (a Seq Map value, serialized as JSON internally)
    /// TODO: Replace with actual seq_runtime::Value once we integrate
    pub state: serde_json::Value,

    /// Behavior quotation reference
    /// This is the name/id of the quotation that handles messages
    pub behavior: String,

    /// Sequence number for event ordering
    pub sequence: u64,
}

impl Actor {
    /// Create a new actor with empty state
    pub fn new(behavior: String) -> Self {
        Actor {
            id: ActorId::new(),
            state: serde_json::json!({}),
            behavior,
            sequence: 0,
        }
    }

    /// Create an actor with a specific ID (for recovery)
    pub fn with_id(id: ActorId, behavior: String) -> Self {
        Actor {
            id,
            state: serde_json::json!({}),
            behavior,
            sequence: 0,
        }
    }

    /// Get the next sequence number and increment
    pub fn next_sequence(&mut self) -> u64 {
        let seq = self.sequence;
        self.sequence += 1;
        seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_id_creation() {
        let id1 = ActorId::new();
        let id2 = ActorId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_actor_creation() {
        let actor = Actor::new("my-behavior".to_string());
        assert_eq!(actor.behavior, "my-behavior");
        assert_eq!(actor.sequence, 0);
    }

    #[test]
    fn test_sequence_increment() {
        let mut actor = Actor::new("test".to_string());
        assert_eq!(actor.next_sequence(), 0);
        assert_eq!(actor.next_sequence(), 1);
        assert_eq!(actor.next_sequence(), 2);
    }
}
