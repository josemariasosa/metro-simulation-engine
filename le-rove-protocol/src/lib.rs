mod identity;
mod message;

pub use identity::{CommandId, NodeId, StationId, TrainId};
pub use message::{Command, EmergencyReason, Message};
