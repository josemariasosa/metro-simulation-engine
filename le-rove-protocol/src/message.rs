use crate::codec::Field;
use crate::{
    DecodeError,
    identity::{StationId, TrainId},
};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Depart = 0x01,
    EmergencyStop = 0x02,
    Accepted = 0x10,
    Rejected = 0x11,
    Departed = 0x20,
    Stopped = 0x21,
}

impl MessageKind {
    pub const fn encoded_len(&self) -> usize {
        match self {
            Self::Depart => 16,
            Self::EmergencyStop => 13,
            Self::Accepted => 13,
            Self::Rejected => 14,
            Self::Departed => 16,
            Self::Stopped => 18,
        }
    }

    pub fn try_from(value: u8) -> Result<Self, DecodeError> {
        match value {
            0x01 => Ok(MessageKind::Depart),
            0x02 => Ok(MessageKind::EmergencyStop),
            0x10 => Ok(MessageKind::Accepted),
            0x11 => Ok(MessageKind::Rejected),
            0x20 => Ok(MessageKind::Departed),
            0x21 => Ok(MessageKind::Stopped),
            _ => Err(DecodeError::UnknownMessageKind { actual: value }),
        }
    }
}

impl From<u8> for MessageKind {
    fn from(value: u8) -> Self {
        MessageKind::try_from(value).expect("Invalid MessageKind value")
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

impl CommandKind {
    pub const fn try_from(value: u8) -> Result<Self, DecodeError> {
        match value {
            0x01 => Ok(CommandKind::Depart),
            0x02 => Ok(CommandKind::EmergencyStop),
            _ => Err(DecodeError::InvalidDiscriminant {
                field: Field::CommandKind,
                actual: value,
            }),
        }
    }
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

impl EmergencyReason {
    pub const fn try_from(value: u8) -> Result<Self, DecodeError> {
        match value {
            0x01 => Ok(EmergencyReason::OperatorRequest),
            0x02 => Ok(EmergencyReason::BrainSafetyDecision),
            _ => Err(DecodeError::InvalidDiscriminant {
                field: Field::EmergencyReason,
                actual: value,
            }),
        }
    }
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

impl RejectionReason {
    pub const fn try_from(value: u8) -> Result<Self, DecodeError> {
        match value {
            0x01 => Ok(RejectionReason::InvalidState),
            0x02 => Ok(RejectionReason::LocationMismatch),
            0x03 => Ok(RejectionReason::UnknownStation),
            0x04 => Ok(RejectionReason::UnsupportedMovement),
            0x05 => Ok(RejectionReason::SafetyInhibit),
            0x06 => Ok(RejectionReason::UnableToExecute),
            _ => Err(DecodeError::InvalidDiscriminant {
                field: Field::RejectionReason,
                actual: value,
            }),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EmergencyCommand = 0x01,
    Obstacle = 0x02,
    LocalStop = 0x03,
    Unspecified = 0x04,
}

impl StopReason {
    pub const fn try_from(value: u8) -> Result<Self, DecodeError> {
        match value {
            0x01 => Ok(StopReason::EmergencyCommand),
            0x02 => Ok(StopReason::Obstacle),
            0x03 => Ok(StopReason::LocalStop),
            0x04 => Ok(StopReason::Unspecified),
            _ => Err(DecodeError::InvalidDiscriminant {
                field: Field::StopReason,
                actual: value,
            }),
        }
    }
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

impl LocationKind {
    pub const fn try_from(value: u8) -> Result<Self, DecodeError> {
        match value {
            0x00 => Ok(LocationKind::Unknown),
            0x01 => Ok(LocationKind::AtStation),
            0x02 => Ok(LocationKind::OnSegment),
            _ => Err(DecodeError::InvalidDiscriminant {
                field: Field::LocationKind,
                actual: value,
            }),
        }
    }
}
