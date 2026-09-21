use crate::station::StationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrainId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrainState {
    AtStation {
        station: StationId,
    },

    Moving {
        from: StationId,
        to: StationId,
        progress: f32,
    },
}

#[derive(Debug)]
pub struct Train {
    pub id: TrainId,
    pub capacity: usize,
    pub state: TrainState,
}

impl Train {
    pub fn new(id: TrainId, capacity: usize, station: StationId) -> Self {
        Self {
            id,
            capacity,
            state: TrainState::AtStation { station },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn train_starts_at_station() {
        let station = StationId(0);

        let train = Train::new(TrainId(0), 100, station);

        assert_eq!(train.state, TrainState::AtStation { station });
    }
}
