use crate::train::TrainId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainCommand {
    Accelerate { train_id: TrainId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    UnknownTrain,
    NotManual,
    AlreadyMoving,
    NoOutgoingTrack,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_error_debug() {
        let error = CommandError::UnknownTrain;
        assert_eq!(format!("{:?}", error), "UnknownTrain");
    }

    #[test]
    fn command_errors_are_comparable() {
        assert_eq!(CommandError::UnknownTrain, CommandError::UnknownTrain);
        assert_ne!(CommandError::UnknownTrain, CommandError::NotManual);
    }

    #[test]
    fn accelerate_command_carries_train_id() {
        let command = TrainCommand::Accelerate {
            train_id: TrainId(7),
        };

        assert_eq!(
            command,
            TrainCommand::Accelerate {
                train_id: TrainId(7),
            }
        );
    }
}
