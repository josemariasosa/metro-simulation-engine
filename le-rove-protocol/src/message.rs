use crate::identity::{StationId, TrainId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Command(Command),
    CommandResult(CommandResult),
    Event(Event),
}

impl Message {
    pub const fn kind(&self) -> MessageKind {
        match self {
            Message::Command(Command::Depart { .. }) => MessageKind::Depart,
            Message::Command(Command::EmergencyStop { .. }) => MessageKind::EmergencyStop,
            Message::CommandResult(CommandResult::Accepted { .. }) => MessageKind::Accepted,
            Message::CommandResult(CommandResult::Rejected { .. }) => MessageKind::Rejected,
            Message::Event(Event::Departed { .. }) => MessageKind::Departed,
            Message::Event(Event::Stopped { .. }) => MessageKind::Stopped,
        }
    }
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

impl MessageKind {
    pub const fn encoded_len(self) -> usize {
        match self {
            Self::Depart => 16,
            Self::EmergencyStop => 13,
            Self::Accepted => 13,
            Self::Rejected => 14,
            Self::Departed => 16,
            Self::Stopped => 18,
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

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Depart = 0x01,
    EmergencyStop = 0x02,
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

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmergencyReason {
    OperatorRequest = 0x01,
    BrainSafetyDecision = 0x02,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    InvalidState = 0x01,
    LocationMismatch = 0x02,
    UnknownStation = 0x03,
    UnsupportedMovement = 0x04,
    SafetyInhibit = 0x05,
    UnableToExecute = 0x06,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EmergencyCommand = 0x01,
    Obstacle = 0x02,
    LocalStop = 0x03,
    Unspecified = 0x04,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Unknown,
    AtStation(StationId),
    OnSegment { from: StationId, to: StationId },
}

impl Location {
    pub const fn kind(&self) -> LocationKind {
        match self {
            Location::Unknown => LocationKind::Unknown,
            Location::AtStation(_) => LocationKind::AtStation,
            Location::OnSegment { .. } => LocationKind::OnSegment,
        }
    }
}

#[repr(u8)]
pub enum LocationKind {
    Unknown = 0x00,
    AtStation = 0x01,
    OnSegment = 0x02,
}
