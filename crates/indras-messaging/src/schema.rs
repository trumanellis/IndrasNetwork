//! Typed message schema system
//!
//! Provides versioning, validation, and forward-compatibility for message content.
//!
//! ## Overview
//!
//! This module implements a schema system that:
//! - Tracks schema versions for forward/backward compatibility
//! - Validates message content against expected schemas
//! - Provides migration helpers for schema evolution
//! - Supports custom typed messages beyond built-in types
//!
//! ## Example
//!
//! ```rust
//! use indras_messaging::schema::{SchemaVersion, TypedContent, ContentValidator};
//!
//! // Define a schema version
//! let v1 = SchemaVersion::new(1, 0);
//!
//! // Create typed content
//! let content = TypedContent::new("chat.message", v1, b"Hello world".to_vec());
//!
//! // Validate with a validator
//! let validator = ContentValidator::default();
//! assert!(validator.validate(&content).is_ok());
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Schema version following semantic versioning principles
///
/// Major version changes indicate breaking changes.
/// Minor version changes indicate backward-compatible additions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// Major version (breaking changes)
    pub major: u16,
    /// Minor version (backward-compatible additions)
    pub minor: u16,
}

impl SchemaVersion {
    /// Current schema version for built-in types
    pub const CURRENT: SchemaVersion = SchemaVersion { major: 1, minor: 0 };

    /// Create a new schema version
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Check if this version is compatible with another version
    ///
    /// Compatible means the same major version and this minor >= other minor.
    pub fn is_compatible_with(&self, other: &SchemaVersion) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    /// Check if this version can read content from another version
    ///
    /// We can read content if:
    /// - Same major version (no breaking changes)
    /// - Our minor version is >= the content's minor version
    pub fn can_read(&self, content_version: &SchemaVersion) -> bool {
        self.is_compatible_with(content_version)
    }

    /// Check if this is a newer version than another
    pub fn is_newer_than(&self, other: &SchemaVersion) -> bool {
        self.major > other.major || (self.major == other.major && self.minor > other.minor)
    }
}

impl Default for SchemaVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Schema errors
#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("Incompatible schema version: expected {expected}, got {actual}")]
    IncompatibleVersion {
        expected: SchemaVersion,
        actual: SchemaVersion,
    },

    #[error("Unknown content type: {0}")]
    UnknownContentType(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid field value: {field} - {reason}")]
    InvalidField { field: String, reason: String },

    #[error("Content too large: {size} bytes (max: {max})")]
    ContentTooLarge { size: usize, max: usize },

    #[error("Deserialization error: {0}")]
    DeserializationError(String),
}

/// Result type for schema operations
pub type SchemaResult<T> = Result<T, SchemaError>;

/// Well-known content type identifiers
pub mod content_types {
    /// Plain text message
    pub const TEXT: &str = "indras.message.text";
    /// Binary data with MIME type
    pub const BINARY: &str = "indras.message.binary";
    /// File reference
    pub const FILE: &str = "indras.message.file";
    /// Reaction to another message
    pub const REACTION: &str = "indras.message.reaction";
    /// System message
    pub const SYSTEM: &str = "indras.message.system";
    /// Custom JSON content
    pub const CUSTOM_JSON: &str = "indras.message.custom.json";
    /// Custom binary content
    pub const CUSTOM_BINARY: &str = "indras.message.custom.binary";
}

/// Typed content with schema information
///
/// This wraps message content with type and version metadata
/// for validation and forward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedContent {
    /// Content type identifier (e.g., "indras.message.text")
    pub content_type: String,
    /// Schema version of the content
    pub schema_version: SchemaVersion,
    /// The actual content data (serialized)
    pub data: Vec<u8>,
    /// Optional metadata (extension fields)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl TypedContent {
    /// Create new typed content
    pub fn new(content_type: impl Into<String>, schema_version: SchemaVersion, data: Vec<u8>) -> Self {
        Self {
            content_type: content_type.into(),
            schema_version,
            data,
            metadata: HashMap::new(),
        }
    }

    /// Create typed content with current schema version
    pub fn with_current_version(content_type: impl Into<String>, data: Vec<u8>) -> Self {
        Self::new(content_type, SchemaVersion::CURRENT, data)
    }

    /// Create a text content
    pub fn text(text: impl AsRef<str>) -> Self {
        Self::with_current_version(content_types::TEXT, text.as_ref().as_bytes().to_vec())
    }

    /// Create a binary content
    pub fn binary(mime_type: &str, data: Vec<u8>) -> Self {
        let mut content = Self::with_current_version(content_types::BINARY, data);
        content.metadata.insert("mime_type".to_string(), mime_type.to_string());
        content
    }

    /// Create a system message
    pub fn system(message: impl AsRef<str>) -> Self {
        Self::with_current_version(content_types::SYSTEM, message.as_ref().as_bytes().to_vec())
    }

    /// Create custom JSON content
    pub fn custom_json<T: Serialize>(type_name: &str, value: &T) -> SchemaResult<Self> {
        let data = serde_json::to_vec(value)
            .map_err(|e| SchemaError::DeserializationError(e.to_string()))?;
        let mut content = Self::with_current_version(content_types::CUSTOM_JSON, data);
        content.metadata.insert("custom_type".to_string(), type_name.to_string());
        Ok(content)
    }

    /// Add metadata to the content
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Get content as text (if it's a text type)
    pub fn as_text(&self) -> Option<&str> {
        if self.content_type == content_types::TEXT || self.content_type == content_types::SYSTEM {
            std::str::from_utf8(&self.data).ok()
        } else {
            None
        }
    }

    /// Get custom JSON content
    pub fn as_custom_json<T: for<'de> Deserialize<'de>>(&self) -> SchemaResult<T> {
        if self.content_type != content_types::CUSTOM_JSON {
            return Err(SchemaError::UnknownContentType(self.content_type.clone()));
        }
        serde_json::from_slice(&self.data)
            .map_err(|e| SchemaError::DeserializationError(e.to_string()))
    }

    /// Get the size of the content data
    pub fn data_size(&self) -> usize {
        self.data.len()
    }

    /// Check if this content type is a built-in type
    pub fn is_builtin_type(&self) -> bool {
        self.content_type.starts_with("indras.message.")
    }
}

/// Content validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum allowed content size in bytes
    pub max_content_size: usize,
    /// Maximum text length in characters
    pub max_text_length: usize,
    /// Allow unknown content types
    pub allow_unknown_types: bool,
    /// Strict version checking (fail on incompatible versions)
    pub strict_versions: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_content_size: 10 * 1024 * 1024, // 10 MB
            max_text_length: 100_000,           // 100K characters
            allow_unknown_types: true,
            strict_versions: false,
        }
    }
}

/// Content validator for typed messages
pub struct ContentValidator {
    config: ValidationConfig,
    /// Custom type validators
    custom_validators: HashMap<String, Box<dyn Fn(&TypedContent) -> SchemaResult<()> + Send + Sync>>,
}

impl ContentValidator {
    /// Create a new validator with default configuration
    pub fn new() -> Self {
        Self {
            config: ValidationConfig::default(),
            custom_validators: HashMap::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ValidationConfig) -> Self {
        Self {
            config,
            custom_validators: HashMap::new(),
        }
    }

    /// Register a custom validator for a content type
    pub fn register_validator<F>(&mut self, content_type: impl Into<String>, validator: F)
    where
        F: Fn(&TypedContent) -> SchemaResult<()> + Send + Sync + 'static,
    {
        self.custom_validators
            .insert(content_type.into(), Box::new(validator));
    }

    /// Validate typed content
    pub fn validate(&self, content: &TypedContent) -> SchemaResult<()> {
        // Check content size
        if content.data_size() > self.config.max_content_size {
            return Err(SchemaError::ContentTooLarge {
                size: content.data_size(),
                max: self.config.max_content_size,
            });
        }

        // Check version compatibility
        if self.config.strict_versions
            && !SchemaVersion::CURRENT.can_read(&content.schema_version)
        {
            return Err(SchemaError::IncompatibleVersion {
                expected: SchemaVersion::CURRENT,
                actual: content.schema_version,
            });
        }

        // Built-in type validation
        match content.content_type.as_str() {
            content_types::TEXT | content_types::SYSTEM => {
                self.validate_text(content)?;
            }
            content_types::BINARY => {
                self.validate_binary(content)?;
            }
            content_types::FILE => {
                self.validate_file(content)?;
            }
            content_types::REACTION => {
                self.validate_reaction(content)?;
            }
            content_types::CUSTOM_JSON => {
                self.validate_custom_json(content)?;
            }
            content_types::CUSTOM_BINARY => {
                // Custom binary has minimal validation
            }
            _ => {
                if !self.config.allow_unknown_types && !content.is_builtin_type() {
                    return Err(SchemaError::UnknownContentType(content.content_type.clone()));
                }
            }
        }

        // Run custom validator if registered
        if let Some(validator) = self.custom_validators.get(&content.content_type) {
            validator(content)?;
        }

        Ok(())
    }

    fn validate_text(&self, content: &TypedContent) -> SchemaResult<()> {
        // Must be valid UTF-8
        let text = std::str::from_utf8(&content.data)
            .map_err(|_| SchemaError::ValidationFailed("Text content must be valid UTF-8".into()))?;

        // Check length
        if text.chars().count() > self.config.max_text_length {
            return Err(SchemaError::InvalidField {
                field: "text".into(),
                reason: format!(
                    "Text exceeds maximum length of {} characters",
                    self.config.max_text_length
                ),
            });
        }

        Ok(())
    }

    fn validate_binary(&self, content: &TypedContent) -> SchemaResult<()> {
        // Must have mime_type in metadata
        if !content.metadata.contains_key("mime_type") {
            return Err(SchemaError::MissingField("mime_type".into()));
        }
        Ok(())
    }

    fn validate_file(&self, content: &TypedContent) -> SchemaResult<()> {
        // File references should have name and size in metadata
        if !content.metadata.contains_key("filename") {
            return Err(SchemaError::MissingField("filename".into()));
        }
        if !content.metadata.contains_key("size") {
            return Err(SchemaError::MissingField("size".into()));
        }
        Ok(())
    }

    fn validate_reaction(&self, content: &TypedContent) -> SchemaResult<()> {
        // Must have target_message_id in metadata
        if !content.metadata.contains_key("target_message_id") {
            return Err(SchemaError::MissingField("target_message_id".into()));
        }
        // Content should be the reaction (emoji or identifier)
        if content.data.is_empty() {
            return Err(SchemaError::ValidationFailed("Reaction cannot be empty".into()));
        }
        Ok(())
    }

    fn validate_custom_json(&self, content: &TypedContent) -> SchemaResult<()> {
        // Must be valid JSON
        let _: serde_json::Value = serde_json::from_slice(&content.data)
            .map_err(|e| SchemaError::ValidationFailed(format!("Invalid JSON: {}", e)))?;
        Ok(())
    }
}

impl Default for ContentValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Schema migration helper for version upgrades
pub struct SchemaMigration {
    /// Source version
    pub from_version: SchemaVersion,
    /// Target version
    pub to_version: SchemaVersion,
    /// Content type this migration applies to
    pub content_type: String,
}

impl SchemaMigration {
    /// Create a new migration definition
    pub fn new(
        content_type: impl Into<String>,
        from_version: SchemaVersion,
        to_version: SchemaVersion,
    ) -> Self {
        Self {
            from_version,
            to_version,
            content_type: content_type.into(),
        }
    }

    /// Check if this migration applies to given content
    pub fn applies_to(&self, content: &TypedContent) -> bool {
        content.content_type == self.content_type
            && content.schema_version == self.from_version
    }
}

/// Schema registry for managing content types and migrations
pub struct SchemaRegistry {
    /// Registered content types with their descriptions
    content_types: HashMap<String, ContentTypeInfo>,
    /// Registered migrations
    migrations: Vec<SchemaMigration>,
}

/// Information about a registered content type
#[derive(Debug, Clone)]
pub struct ContentTypeInfo {
    /// Content type identifier
    pub type_id: String,
    /// Human-readable description
    pub description: String,
    /// Current schema version
    pub current_version: SchemaVersion,
    /// Minimum supported version for reading
    pub min_supported_version: SchemaVersion,
}

impl SchemaRegistry {
    /// Create a new registry with built-in types
    pub fn new() -> Self {
        let mut registry = Self {
            content_types: HashMap::new(),
            migrations: Vec::new(),
        };

        // Register built-in types
        registry.register_builtin_types();

        registry
    }

    fn register_builtin_types(&mut self) {
        let builtin = [
            (content_types::TEXT, "Plain text message"),
            (content_types::BINARY, "Binary data with MIME type"),
            (content_types::FILE, "File reference"),
            (content_types::REACTION, "Reaction to another message"),
            (content_types::SYSTEM, "System message"),
            (content_types::CUSTOM_JSON, "Custom JSON content"),
            (content_types::CUSTOM_BINARY, "Custom binary content"),
        ];

        for (type_id, description) in builtin {
            self.content_types.insert(
                type_id.to_string(),
                ContentTypeInfo {
                    type_id: type_id.to_string(),
                    description: description.to_string(),
                    current_version: SchemaVersion::CURRENT,
                    min_supported_version: SchemaVersion::new(1, 0),
                },
            );
        }
    }

    /// Register a custom content type
    pub fn register_type(&mut self, info: ContentTypeInfo) {
        self.content_types.insert(info.type_id.clone(), info);
    }

    /// Get information about a content type
    pub fn get_type_info(&self, type_id: &str) -> Option<&ContentTypeInfo> {
        self.content_types.get(type_id)
    }

    /// Check if a content type is registered
    pub fn is_registered(&self, type_id: &str) -> bool {
        self.content_types.contains_key(type_id)
    }

    /// Get all registered content types
    pub fn all_types(&self) -> impl Iterator<Item = &ContentTypeInfo> {
        self.content_types.values()
    }

    /// Register a migration
    pub fn register_migration(&mut self, migration: SchemaMigration) {
        self.migrations.push(migration);
    }

    /// Find applicable migrations for content
    pub fn find_migrations(&self, content: &TypedContent) -> Vec<&SchemaMigration> {
        self.migrations
            .iter()
            .filter(|m| m.applies_to(content))
            .collect()
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_compatibility() {
        let v1_0 = SchemaVersion::new(1, 0);
        let v1_1 = SchemaVersion::new(1, 1);
        let v2_0 = SchemaVersion::new(2, 0);

        // Same version is compatible
        assert!(v1_0.is_compatible_with(&v1_0));

        // Newer minor can read older minor
        assert!(v1_1.is_compatible_with(&v1_0));
        assert!(v1_1.can_read(&v1_0));

        // Older minor cannot read newer minor
        assert!(!v1_0.can_read(&v1_1));

        // Different major versions are incompatible
        assert!(!v2_0.is_compatible_with(&v1_0));
        assert!(!v1_0.is_compatible_with(&v2_0));
    }

    #[test]
    fn test_schema_version_ordering() {
        let v1_0 = SchemaVersion::new(1, 0);
        let v1_1 = SchemaVersion::new(1, 1);
        let v2_0 = SchemaVersion::new(2, 0);

        assert!(v1_1.is_newer_than(&v1_0));
        assert!(v2_0.is_newer_than(&v1_1));
        assert!(!v1_0.is_newer_than(&v1_1));
    }

    #[test]
    fn test_typed_content_text() {
        let content = TypedContent::text("Hello, world!");

        assert_eq!(content.content_type, content_types::TEXT);
        assert_eq!(content.as_text(), Some("Hello, world!"));
        assert!(content.is_builtin_type());
    }

    #[test]
    fn test_typed_content_binary() {
        let data = vec![1, 2, 3, 4];
        let content = TypedContent::binary("application/octet-stream", data.clone());

        assert_eq!(content.content_type, content_types::BINARY);
        assert_eq!(content.data, data);
        assert_eq!(
            content.metadata.get("mime_type"),
            Some(&"application/octet-stream".to_string())
        );
    }

    #[test]
    fn test_typed_content_custom_json() {
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct CustomData {
            value: i32,
            name: String,
        }

        let original = CustomData {
            value: 42,
            name: "test".to_string(),
        };

        let content = TypedContent::custom_json("my.custom.type", &original).unwrap();
        let recovered: CustomData = content.as_custom_json().unwrap();

        assert_eq!(original, recovered);
    }

    #[test]
    fn test_content_validator_text() {
        let validator = ContentValidator::new();
        let content = TypedContent::text("Hello");

        assert!(validator.validate(&content).is_ok());
    }

    #[test]
    fn test_content_validator_too_large() {
        let config = ValidationConfig {
            max_content_size: 10,
            ..Default::default()
        };
        let validator = ContentValidator::with_config(config);

        let content = TypedContent::text("This is more than 10 bytes");

        match validator.validate(&content) {
            Err(SchemaError::ContentTooLarge { .. }) => {}
            other => panic!("Expected ContentTooLarge error, got {:?}", other),
        }
    }

    #[test]
    fn test_content_validator_binary_missing_mime() {
        let validator = ContentValidator::new();

        // Create binary content without mime_type
        let content = TypedContent::with_current_version(content_types::BINARY, vec![1, 2, 3]);

        match validator.validate(&content) {
            Err(SchemaError::MissingField(field)) => {
                assert_eq!(field, "mime_type");
            }
            other => panic!("Expected MissingField error, got {:?}", other),
        }
    }

    #[test]
    fn test_content_validator_custom_validator() {
        let mut validator = ContentValidator::new();

        // Register a custom validator that rejects content containing "forbidden"
        validator.register_validator(content_types::TEXT, |content| {
            if let Some(text) = content.as_text() {
                if text.contains("forbidden") {
                    return Err(SchemaError::ValidationFailed("Content contains forbidden word".into()));
                }
            }
            Ok(())
        });

        let good = TypedContent::text("Hello");
        let bad = TypedContent::text("This is forbidden");

        assert!(validator.validate(&good).is_ok());
        assert!(validator.validate(&bad).is_err());
    }

    #[test]
    fn test_schema_registry() {
        let registry = SchemaRegistry::new();

        // Built-in types should be registered
        assert!(registry.is_registered(content_types::TEXT));
        assert!(registry.is_registered(content_types::BINARY));
        assert!(registry.is_registered(content_types::FILE));

        // Custom types should not be registered
        assert!(!registry.is_registered("my.custom.type"));
    }

    #[test]
    fn test_schema_registry_custom_type() {
        let mut registry = SchemaRegistry::new();

        registry.register_type(ContentTypeInfo {
            type_id: "my.custom.type".to_string(),
            description: "My custom content type".to_string(),
            current_version: SchemaVersion::new(1, 0),
            min_supported_version: SchemaVersion::new(1, 0),
        });

        assert!(registry.is_registered("my.custom.type"));

        let info = registry.get_type_info("my.custom.type").unwrap();
        assert_eq!(info.description, "My custom content type");
    }

    #[test]
    fn test_schema_migration_applies() {
        let migration = SchemaMigration::new(
            content_types::TEXT,
            SchemaVersion::new(1, 0),
            SchemaVersion::new(1, 1),
        );

        let content_v1 = TypedContent::new(content_types::TEXT, SchemaVersion::new(1, 0), vec![]);
        let content_v1_1 = TypedContent::new(content_types::TEXT, SchemaVersion::new(1, 1), vec![]);

        assert!(migration.applies_to(&content_v1));
        assert!(!migration.applies_to(&content_v1_1));
    }

    #[test]
    fn test_version_display() {
        let v = SchemaVersion::new(1, 5);
        assert_eq!(format!("{}", v), "1.5");
    }

    #[test]
    fn test_content_with_metadata() {
        let content = TypedContent::text("Hello")
            .with_metadata("key1", "value1")
            .with_metadata("key2", "value2");

        assert_eq!(content.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(content.metadata.get("key2"), Some(&"value2".to_string()));
    }
}
