mod decode;
mod encode;
mod roundtrip;

use super::*;

use crate::frame::Frame;
use crate::identity::{CommandId, NodeId, StationId, TrainId};
use crate::message::{
    Command, CommandKind, CommandResult, EmergencyReason, Event, Location, Message,
    RejectionReason, StopReason,
};

const DEPART_ID: u32 = 0x01020304;
const EMERGENCY_ID: u32 = 0x01020305;

const DEPART: [u8; 16] = [
    0x01, 0x01, 0x01, 0x00, 0x02, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x10, 0x00, 0x20, 0x00,
];

const EMERGENCY: [u8; 13] = [
    0x01, 0x02, 0x01, 0x00, 0x02, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01,
];

const ACCEPTED_DEPART: [u8; 13] = [
    0x01, 0x10, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01,
];

const ACCEPTED_EMERGENCY: [u8; 13] = [
    0x01, 0x10, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x02,
];

const REJECTED_DEPART: [u8; 14] = [
    0x01, 0x11, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01, 0x02,
];

const REJECTED_EMERGENCY: [u8; 14] = [
    0x01, 0x11, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x02, 0x06,
];

const DEPARTED: [u8; 16] = [
    0x01, 0x20, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x10, 0x00, 0x20, 0x00,
];

const STOPPED_EMERGENCY: [u8; 18] = [
    0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01, 0x02, 0x10, 0x00,
    0x20, 0x00,
];

const STOPPED_OBSTACLE: [u8; 18] = [
    0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x02, 0x02, 0x10, 0x00,
    0x20, 0x00,
];

const STOPPED_STATION: [u8; 18] = [
    0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x03, 0x01, 0x10, 0x00,
    0x00, 0x00,
];

const STOPPED_UNKNOWN: [u8; 18] = [
    0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00,
    0x00, 0x00,
];

fn node(value: u16) -> NodeId {
    NodeId::new(value).unwrap()
}

fn train(value: u16) -> TrainId {
    TrainId::new(value).unwrap()
}

fn station(value: u16) -> StationId {
    StationId::new(value).unwrap()
}

fn command_id(value: u32) -> Option<CommandId> {
    Some(CommandId::new(value).unwrap())
}

fn frame(message: Message, command_id: Option<CommandId>, reply: bool) -> Frame {
    Frame {
        source: node(if reply { 2 } else { 1 }),
        destination: node(if reply { 1 } else { 2 }),
        command_id,
        message,
    }
}

fn segment() -> Location {
    Location::OnSegment {
        from: station(16),
        to: station(32),
    }
}

fn depart() -> Frame {
    frame(
        Message::Command(Command::Depart {
            train_id: train(3),
            from: station(16),
            to: station(32),
        }),
        command_id(DEPART_ID),
        false,
    )
}

fn emergency_stop() -> Frame {
    frame(
        Message::Command(Command::EmergencyStop {
            train_id: train(3),
            reason: EmergencyReason::OperatorRequest,
        }),
        command_id(EMERGENCY_ID),
        false,
    )
}

fn accepted(kind: CommandKind, id: u32) -> Frame {
    frame(
        Message::CommandResult(CommandResult::Accepted {
            train_id: train(3),
            command_kind: kind,
        }),
        command_id(id),
        true,
    )
}

fn rejected(kind: CommandKind, reason: RejectionReason, id: u32) -> Frame {
    frame(
        Message::CommandResult(CommandResult::Rejected {
            train_id: train(3),
            command_kind: kind,
            reason,
        }),
        command_id(id),
        true,
    )
}

fn departed() -> Frame {
    frame(
        Message::Event(Event::Departed {
            train_id: train(3),
            from: station(16),
            to: station(32),
        }),
        command_id(DEPART_ID),
        true,
    )
}

fn stopped(reason: StopReason, location: Location, id: Option<CommandId>) -> Frame {
    frame(
        Message::Event(Event::Stopped {
            train_id: train(3),
            reason,
            location,
        }),
        id,
        true,
    )
}
