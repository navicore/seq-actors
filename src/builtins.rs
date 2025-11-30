//! Actor builtins for the Seq compiler
//!
//! This module provides the compiler configuration to register
//! actor-related builtins with the Seq compiler.
//!
//! # Usage
//!
//! ```rust,ignore
//! use seq_actors::compiler_config;
//! use seqc::compile_file_with_config;
//!
//! let config = compiler_config();
//! compile_file_with_config(source, output, false, &config)?;
//! ```

use seqc::config::{CompilerConfig, ExternalBuiltin};

/// Get the compiler configuration with actor builtins registered
///
/// This configuration can be passed to `seqc::compile_file_with_config`
/// to enable actor-related words in Seq programs.
pub fn compiler_config() -> CompilerConfig {
    CompilerConfig::new()
        // Actor lifecycle
        .with_builtin(ExternalBuiltin::new(
            "actor-spawn",      // ( Behavior -- ActorId )
            "seq_actors_spawn",
        ))
        .with_builtin(ExternalBuiltin::new(
            "actor-send",       // ( ActorId Msg -- )
            "seq_actors_send",
        ))
        .with_builtin(ExternalBuiltin::new(
            "actor-self",       // ( -- ActorId )
            "seq_actors_self",
        ))
        .with_builtin(ExternalBuiltin::new(
            "actor-stop",       // ( ActorId -- )
            "seq_actors_stop",
        ))
        // State access (within actor context)
        .with_builtin(ExternalBuiltin::new(
            "actor-state",      // ( -- State )
            "seq_actors_state",
        ))
        // Journal operations
        .with_builtin(ExternalBuiltin::new(
            "journal-append",   // ( Event -- )
            "seq_actors_journal_append",
        ))
        .with_library("seq_actors_runtime")
}

/// Get a minimal config for testing (no library linking)
#[cfg(test)]
pub fn test_config() -> CompilerConfig {
    CompilerConfig::new()
        .with_builtin(ExternalBuiltin::new("actor-spawn", "seq_actors_spawn"))
        .with_builtin(ExternalBuiltin::new("actor-send", "seq_actors_send"))
        .with_builtin(ExternalBuiltin::new("actor-self", "seq_actors_self"))
        .with_builtin(ExternalBuiltin::new("actor-state", "seq_actors_state"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_config_has_builtins() {
        let config = compiler_config();
        assert!(!config.external_builtins.is_empty());

        let names: Vec<&str> = config
            .external_builtins
            .iter()
            .map(|b| b.seq_name.as_str())
            .collect();

        assert!(names.contains(&"actor-spawn"));
        assert!(names.contains(&"actor-send"));
        assert!(names.contains(&"actor-self"));
        assert!(names.contains(&"actor-state"));
    }

    #[test]
    fn test_symbols_are_valid() {
        let config = compiler_config();
        for builtin in &config.external_builtins {
            // Symbols should only contain valid characters
            for c in builtin.symbol.chars() {
                assert!(
                    c.is_alphanumeric() || c == '_',
                    "Invalid char '{}' in symbol '{}'",
                    c,
                    builtin.symbol
                );
            }
        }
    }
}
