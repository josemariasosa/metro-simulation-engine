use crate::domain::constraint::{ConstraintId, ConstraintOrigin, OperationalConstraint};
use crate::domain::station::StationId;
use crate::domain::train::Direction;
use crate::domain::train::TrainId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationSnapshot {
    pub elapsed_seconds: u64,
    pub trains: Vec<TrainSnapshot>,
    pub constraints: Vec<ConstraintSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSnapshot {
    pub id: ConstraintId,
    pub constraint: OperationalConstraint,
    pub start_at: u64,
    pub end_at: Option<u64>,
    pub origin: ConstraintOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainSnapshot {
    pub id: TrainId,
    pub direction: Direction,
    pub state: TrainSnapshotState,
    pub velocity: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrainSnapshotState {
    Dwelling {
        station: StationId,
        remaining_seconds: u64,
    },
    Ready {
        station: StationId,
    },
    Moving {
        from: StationId,
        to: StationId,
        elapsed_seconds: u64,
        travel_seconds: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::station::StationId;

    #[test]
    fn snapshot_dtos_are_owned_and_comparable() {
        let snapshot = SimulationSnapshot {
            elapsed_seconds: 0,
            trains: vec![TrainSnapshot {
                id: TrainId(1),
                direction: Direction::Forward,
                velocity: 0,
                state: TrainSnapshotState::Dwelling {
                    station: StationId(0),
                    remaining_seconds: 3,
                },
            }],
            constraints: vec![],
        };

        let cloned = snapshot.clone();

        assert_eq!(snapshot, cloned);
    }

    #[test]
    fn snapshot_dtos_can_represent_current_train_states() {
        let dwelling = TrainSnapshot {
            id: TrainId(1),
            direction: Direction::Forward,
            velocity: 0,
            state: TrainSnapshotState::Dwelling {
                station: StationId(0),
                remaining_seconds: 3,
            },
        };

        let moving = TrainSnapshot {
            id: TrainId(2),
            direction: Direction::Backward,
            velocity: 1,
            state: TrainSnapshotState::Moving {
                from: StationId(1),
                to: StationId(0),
                elapsed_seconds: 2,
                travel_seconds: 6,
            },
        };

        let snapshot = SimulationSnapshot {
            elapsed_seconds: 5,
            trains: vec![dwelling, moving],
            constraints: vec![],
        };

        let copy = snapshot.clone();

        assert_eq!(snapshot, copy);
    }
}
