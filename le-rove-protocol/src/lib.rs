#![no_std]

mod codec;
mod constants;
mod frame;
mod identity;
mod message;

pub use frame::Frame;
pub use identity::{CommandId, NodeId, StationId, TrainId};
pub use message::{Command, EmergencyReason, Message};

pub use codec::{DecodeError, EncodeError, decode, encode};
