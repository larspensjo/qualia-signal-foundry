use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    #[default]
    Idle,
    AwaitingResponse,
    ToolLoop,
}
