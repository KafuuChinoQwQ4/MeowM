//! Editable transaction categories and classification rule identifiers.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CategoryId(Uuid);

impl CategoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for CategoryId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    /// Disabled categories remain available on historical transactions but cannot
    /// receive new classification rules.
    pub enabled: bool,
}

/// Stable identifier for a user-defined classification rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(Uuid);

impl RuleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for RuleId {
    fn default() -> Self {
        Self::new()
    }
}
