use crate::identity::{CommandId, NodeId};
use crate::message::{Event, Message, StopReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub source: NodeId,
    pub destination: NodeId,
    pub command_id: Option<CommandId>,
    pub message: Message,
}

impl Frame {
    pub(crate) fn has_valid_command_id(&self) -> bool {
        valid_command_id(&self.message, self.command_id)
    }
}

pub(crate) fn valid_command_id(message: &Message, command_id: Option<CommandId>) -> bool {
    match message {
        Message::Event(Event::Stopped {
            reason: StopReason::EmergencyCommand,
            ..
        }) => command_id.is_some(),

        Message::Event(Event::Stopped { .. }) => command_id.is_none(),

        _ => command_id.is_some(),
    }
}
