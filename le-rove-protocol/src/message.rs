use crate::identity::{StationId, TrainId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Command(Command),
    // Las otras categorías vendrán después.
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
pub enum EmergencyReason {
    OperatorRequest,
    BrainSafetyDecision,
}
