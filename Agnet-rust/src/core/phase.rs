use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};

/// 透明的阶段标识符
///
/// 核心不做任何语义假设，仅作为阶段名称
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase(String);

impl Serialize for Phase {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Phase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Phase(s))
    }
}

impl Phase {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    // Phase 常量方法
    pub fn init() -> Self {
        Self("init".to_string())
    }
    pub fn context() -> Self {
        Self("context".to_string())
    }
    pub fn think() -> Self {
        Self("think".to_string())
    }
    pub fn audit() -> Self {
        Self("audit".to_string())
    }
    pub fn execute() -> Self {
        Self("execute".to_string())
    }
    pub fn loop_phase() -> Self {
        Self("loop".to_string())
    }
    pub fn memorize() -> Self {
        Self("memorize".to_string())
    }
}

impl Hash for Phase {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_new() {
        let p = Phase::new("custom");
        assert_eq!(p.as_str(), "custom");
        assert_eq!(p.to_string(), "custom");
    }

    #[test]
    fn test_phase_constants() {
        assert_eq!(Phase::init().as_str(), "init");
        assert_eq!(Phase::context().as_str(), "context");
        assert_eq!(Phase::think().as_str(), "think");
        assert_eq!(Phase::audit().as_str(), "audit");
        assert_eq!(Phase::execute().as_str(), "execute");
        assert_eq!(Phase::loop_phase().as_str(), "loop");
        assert_eq!(Phase::memorize().as_str(), "memorize");
    }

    #[test]
    fn test_phase_equality() {
        let a = Phase::new("test");
        let b = Phase::new("test");
        let c = Phase::new("other");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_phase_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Phase::new("a"));
        set.insert(Phase::new("b"));
        set.insert(Phase::new("a"));
        assert_eq!(set.len(), 2);
    }
}
