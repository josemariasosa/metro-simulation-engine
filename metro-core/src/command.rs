use crate::train::TrainId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainCommand {
    Accelerate { train_id: TrainId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    UnknownTrain,
    NotManual,
    NoOutgoingTrack,
    Blocked,
}
