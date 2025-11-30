//! Serialization of Seq Values for actor persistence
//!
//! This module handles converting Seq runtime values to binary format for journal storage.
//!
//! # Why bincode?
//!
//! The journal is an internal implementation detail for actor resurrection.
//! External systems access history through the actor's API, not raw journal data.
//! Therefore we optimize for:
//! - Speed (fast writes during operation, fast reads on recovery)
//! - Compactness (less disk I/O, smaller storage)
//!
//! # Why not Seq stdlib JSON?
//!
//! The seq stdlib `json` module is for Seq programs to parse/emit JSON at runtime.
//! Here we're serializing `Value` types from Rust (the actor runtime), not from
//! within Seq code. Using serde/bincode makes sense at this layer.
//!
//! # Type Mapping
//!
//! | Seq Type   | Serialization |
//! |------------|---------------|
//! | Int        | i64 |
//! | Float      | f64 |
//! | Bool       | bool |
//! | String     | length-prefixed bytes |
//! | Map        | encoded key-value pairs |
//! | Variant    | tag string + field array |
//! | Quotation  | **Error** - cannot serialize code |
//!
//! # Debugging
//!
//! Use `TypedValue::to_debug_string()` for human-readable inspection.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Error during serialization
#[derive(Debug)]
pub enum SerializeError {
    /// Cannot serialize quotations (code)
    QuotationNotSerializable,
    /// Cannot serialize closures
    ClosureNotSerializable,
    /// Bincode encoding/decoding error
    BincodeError(String),
    /// Invalid data structure
    InvalidData(String),
}

impl std::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializeError::QuotationNotSerializable => {
                write!(f, "Quotations cannot be serialized - actor state must be data only")
            }
            SerializeError::ClosureNotSerializable => {
                write!(f, "Closures cannot be serialized - actor state must be data only")
            }
            SerializeError::BincodeError(msg) => write!(f, "Bincode error: {}", msg),
            SerializeError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
        }
    }
}

impl std::error::Error for SerializeError {}

impl From<bincode::Error> for SerializeError {
    fn from(e: bincode::Error) -> Self {
        SerializeError::BincodeError(e.to_string())
    }
}

/// Typed value for serialization (mirrors Seq runtime Value)
///
/// This is the serializable representation of Seq values.
/// Quotations and closures cannot be represented here - they're code, not data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TypedValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    /// Map with typed keys and values
    /// Keys are encoded as TypedValue to preserve type information
    Map(BTreeMap<MapKey, TypedValue>),
    /// Variant with tag and fields
    Variant { tag: String, fields: Vec<TypedValue> },
}

/// Map key types (subset of TypedValue that can be keys)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapKey {
    Int(i64),
    Bool(bool),
    String(String),
}

impl MapKey {
    /// Convert to a TypedValue
    pub fn to_typed_value(&self) -> TypedValue {
        match self {
            MapKey::Int(v) => TypedValue::Int(*v),
            MapKey::Bool(v) => TypedValue::Bool(*v),
            MapKey::String(v) => TypedValue::String(v.clone()),
        }
    }
}

impl TypedValue {
    /// Try to convert to a map key (fails for Float, Map, Variant)
    pub fn to_map_key(&self) -> Result<MapKey, SerializeError> {
        match self {
            TypedValue::Int(v) => Ok(MapKey::Int(*v)),
            TypedValue::Bool(v) => Ok(MapKey::Bool(*v)),
            TypedValue::String(v) => Ok(MapKey::String(v.clone())),
            TypedValue::Float(_) => Err(SerializeError::InvalidData(
                "Float cannot be a map key".to_string(),
            )),
            TypedValue::Map(_) => Err(SerializeError::InvalidData(
                "Map cannot be a map key".to_string(),
            )),
            TypedValue::Variant { .. } => Err(SerializeError::InvalidData(
                "Variant cannot be a map key".to_string(),
            )),
        }
    }

    /// Serialize to binary format
    pub fn to_bytes(&self) -> Result<Vec<u8>, SerializeError> {
        bincode::serialize(self).map_err(SerializeError::from)
    }

    /// Deserialize from binary format
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SerializeError> {
        bincode::deserialize(bytes).map_err(SerializeError::from)
    }

    /// Convert to human-readable debug string
    pub fn to_debug_string(&self) -> String {
        match self {
            TypedValue::Int(v) => format!("{}", v),
            TypedValue::Float(v) => format!("{}", v),
            TypedValue::Bool(v) => format!("{}", v),
            TypedValue::String(v) => format!("{:?}", v),
            TypedValue::Map(m) => {
                let entries: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{}: {}", key_to_debug_string(k), v.to_debug_string()))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
            TypedValue::Variant { tag, fields } => {
                if fields.is_empty() {
                    tag.clone()
                } else {
                    let field_strs: Vec<String> =
                        fields.iter().map(|f| f.to_debug_string()).collect();
                    format!("({} {})", tag, field_strs.join(" "))
                }
            }
        }
    }
}

fn key_to_debug_string(key: &MapKey) -> String {
    match key {
        MapKey::Int(v) => format!("{}", v),
        MapKey::Bool(v) => format!("{}", v),
        MapKey::String(v) => format!("{:?}", v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_roundtrip() {
        let value = TypedValue::Int(42);
        let bytes = value.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_string_roundtrip() {
        let value = TypedValue::String("hello".to_string());
        let bytes = value.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_float_roundtrip() {
        let value = TypedValue::Float(3.14159);
        let bytes = value.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_map_roundtrip() {
        let mut map = BTreeMap::new();
        map.insert(MapKey::String("balance".to_string()), TypedValue::Int(500));
        map.insert(MapKey::Int(42), TypedValue::Bool(true));

        let value = TypedValue::Map(map);
        let bytes = value.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_variant_roundtrip() {
        let value = TypedValue::Variant {
            tag: "Deposit".to_string(),
            fields: vec![TypedValue::Int(100)],
        };
        let bytes = value.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_nested_structure_roundtrip() {
        let mut inner_map = BTreeMap::new();
        inner_map.insert(MapKey::String("x".to_string()), TypedValue::Int(10));

        let mut outer_map = BTreeMap::new();
        outer_map.insert(
            MapKey::String("nested".to_string()),
            TypedValue::Map(inner_map),
        );
        outer_map.insert(
            MapKey::String("variant".to_string()),
            TypedValue::Variant {
                tag: "Some".to_string(),
                fields: vec![TypedValue::String("data".to_string())],
            },
        );

        let value = TypedValue::Map(outer_map);
        let bytes = value.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_debug_string() {
        let value = TypedValue::Variant {
            tag: "Deposit".to_string(),
            fields: vec![TypedValue::Int(100)],
        };
        assert_eq!(value.to_debug_string(), "(Deposit 100)");

        let value = TypedValue::String("hello".to_string());
        assert_eq!(value.to_debug_string(), "\"hello\"");
    }

    #[test]
    fn test_bincode_is_compact() {
        // Verify bincode produces reasonably compact output
        let value = TypedValue::Int(42);
        let bytes = value.to_bytes().unwrap();
        // bincode for an enum variant + i64 should be small
        assert!(bytes.len() < 20, "Expected compact encoding, got {} bytes", bytes.len());
    }
}
