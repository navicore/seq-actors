//! seq-actors: Actor system for the Seq programming language
//!
//! This crate extends the Seq compiler with actor-based concurrency
//! and event-sourced persistence.
//!
//! # Architecture
//!
//! - **Actors**: Isolated units with identity, state (Map), and behavior (Quotation)
//! - **Messages**: Variants sent between actors
//! - **Journal**: Binary event log for persistence and recovery
//! - **Supervisor**: Manages actor lifecycle and failure recovery
//!
//! # Serialization
//!
//! The journal uses bincode for fast, compact binary serialization.
//! This is an internal format - external systems access actor history
//! through the actor's API, not by reading the journal directly.
//!
//! For debugging, use `TypedValue::to_debug_string()` or `Journal::dump_debug()`.
//!
//! # Usage
//!
//! ```rust,ignore
//! use seq_actors::compiler_config;
//!
//! // Get compiler config with actor builtins
//! let config = compiler_config();
//!
//! // Compile Seq code that uses actors
//! seqc::compile_file_with_config(source, output, false, &config)?;
//! ```

pub mod actor;
pub mod builtins;
pub mod ffi;
pub mod journal;
pub mod runtime;
pub mod serialize;

// Re-exports
pub use actor::{Actor, ActorId, ActorRef};
pub use builtins::compiler_config;
pub use journal::{Event, Journal, Snapshot};
pub use runtime::{ActorRuntime, Mailbox, RuntimeConfig};

// Serialization re-exports from seq-runtime
pub use serialize::{MapKey, SerializeError, TypedMapKey, TypedValue, ValueSerialize};
