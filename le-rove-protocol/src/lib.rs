#![no_std]

mod codec;
mod constants;
mod frame;
mod identity;
mod message;

pub use codec::{DecodeError, EncodeError, Field, decode, encode};
pub use frame::Frame;
pub use identity::{CommandId, NodeId, StationId, TrainId};
pub use message::{
    Command, CommandKind, CommandResult, EmergencyReason, Event, Location, Message, MessageKind,
    RejectionReason, StopReason,
};
