mod codec;
mod constants;
mod frame;
mod identity;
mod message;

pub use frame::Frame;
pub use identity::{CommandId, NodeId, StationId, TrainId};
pub use message::{Command, EmergencyReason, Message};

pub use codec::{EncodeError, encode};

#[cfg(test)]
pub fn departure_frame() -> Frame {
    Frame {
        source: NodeId::new(1).unwrap(),
        destination: NodeId::new(2).unwrap(),
        command_id: CommandId::new(0x01020304),
        message: Message::Command(Command::Depart {
            train_id: TrainId::new(3).unwrap(),
            from: StationId::new(16).unwrap(),
            to: StationId::new(32).unwrap(),
        }),
    }
}
