use crate::identity::{CommandId, StationId, TrainId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Command(Command),
    CommandResult(CommandResult),
    Event(Event),
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
