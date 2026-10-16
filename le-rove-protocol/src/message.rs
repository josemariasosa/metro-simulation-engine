use crate::identity::{StationId, TrainId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Command(Command),
    CommandResult(CommandResult),
    Event(Event),
}

#[repr(u8)]
pub enum MessageKind {
    Depart = 0x01,
    EmergencyStop = 0x02,
    Accepted = 0x10,
    Rejected = 0x11,
    Departed = 0x20,
    Stopped = 0x21,
}

impl Message {
    pub fn kind(&self) -> MessageKind {
        match self {
            Message::Command(Command::Depart { .. }) => MessageKind::Depart,

            Message::Command(Command::EmergencyStop { .. }) => MessageKind::EmergencyStop,

            // Las otras cuatro variantes vendrán después.
            _ => todo!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Depart {
        train_id: TrainId,
        from: StationId,
        to: StationId,
    },
    EmergencyStop {
        train_id: TrainId,
        reason: EmergencyReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Depart,
    EmergencyStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandResult {
    Accepted {
        train_id: TrainId,
        command_kind: CommandKind,
    },
    Rejected {
        train_id: TrainId,
        command_kind: CommandKind,
        reason: RejectionReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Departed {
        train_id: TrainId,
        from: StationId,
        to: StationId,
    },
    Stopped {
        train_id: TrainId,
        reason: StopReason,
        location: Location,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmergencyReason {
    OperatorRequest,
    BrainSafetyDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    InvalidState,
    LocationMismatch,
    UnknownStation,
    UnsupportedMovement,
    SafetyInhibit,
    UnableToExecute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EmergencyCommand,
    Obstacle,
    LocalStop,
    Unspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Unknown,
    AtStation(StationId),
    OnSegment { from: StationId, to: StationId },
}
