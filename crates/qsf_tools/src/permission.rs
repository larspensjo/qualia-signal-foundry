use serde::{Deserialize, Serialize};

pub use qsf_session::tools::{ToolCategory, ToolSideEffectLevel};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolPermission {
    pub allowed_categories: Vec<ToolCategory>,
    pub max_side_effect_level: ToolSideEffectLevel,
}

impl ToolPermission {
    pub fn compute_only() -> Self {
        Self {
            allowed_categories: vec![ToolCategory::ComputeOnly],
            max_side_effect_level: ToolSideEffectLevel::None,
        }
    }

    pub fn read_only() -> Self {
        Self {
            allowed_categories: vec![ToolCategory::ReadOnly],
            max_side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    pub fn allows(&self, category: ToolCategory, side_effect_level: ToolSideEffectLevel) -> bool {
        self.allowed_categories.contains(&category)
            && side_effect_level <= self.max_side_effect_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_permission_allows_read_only_tools() {
        let permission = ToolPermission::read_only();
        assert!(permission.allows(ToolCategory::ReadOnly, ToolSideEffectLevel::ReadOnly));
    }

    #[test]
    fn read_only_permission_rejects_write_tools() {
        let permission = ToolPermission::read_only();
        assert!(!permission.allows(
            ToolCategory::WriteCapable,
            ToolSideEffectLevel::ExternalWrite
        ));
    }

    #[test]
    fn read_only_permission_rejects_compute_only_category() {
        let permission = ToolPermission::read_only();
        assert!(!permission.allows(ToolCategory::ComputeOnly, ToolSideEffectLevel::None));
    }
}
