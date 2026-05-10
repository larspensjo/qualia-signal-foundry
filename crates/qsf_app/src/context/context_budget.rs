use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextBudget {
    pub max_fragments: usize,
    pub max_estimated_tokens: usize,
}

impl ContextBudget {
    pub fn new(max_fragments: usize, max_estimated_tokens: usize) -> Self {
        Self {
            max_fragments,
            max_estimated_tokens,
        }
    }
}
