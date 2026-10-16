use crate::identity::{CommandId, NodeId};
use crate::message::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub source: NodeId,
    pub destination: NodeId,
    pub command_id: Option<CommandId>,
    pub message: Message,
}
