//! Serialization types for actor persistence
//!
//! Re-exports serialization types from seq-runtime.
//! See seq-runtime::serialize for full documentation.

// Re-export all serialization types from seq-runtime
pub use seq_runtime::{SerializeError, TypedMapKey, TypedValue, ValueSerialize};

// For backwards compatibility, also export MapKey as an alias
pub use TypedMapKey as MapKey;
