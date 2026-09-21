use crate::station::StationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrainId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

impl Direction {
    pub fn reverse(self) -> Self {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrainState {
    AtStation {
        station: StationId,
    },

    Moving {
        from: StationId,
        to: StationId,
        elapsed_seconds: u64,
    },
}

#[derive(Debug)]
pub struct Train {
    pub id: TrainId,
    pub capacity: usize,
    pub state: TrainState,
    pub direction: Direction,
}

impl Train {
    pub fn new(id: TrainId, capacity: usize, station: StationId, direction: Direction) -> Self {
        Self {
            id,
            capacity,
            state: TrainState::AtStation { station },
            direction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn train_starts_at_station() {
        let station = StationId(0);

        let train = Train::new(TrainId(0), 100, station, Direction::Forward);

        assert_eq!(train.state, TrainState::AtStation { station });
    }

    #[test]
    fn direction_can_be_reversed() {
        assert_eq!(Direction::Forward.reverse(), Direction::Backward);

        assert_eq!(Direction::Backward.reverse(), Direction::Forward);
    }
}
