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
    use crate::network::Network;
    use crate::simulation::Simulation;

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

    #[test]
    fn moving_train_advances_elapsed_time() {
        let mut network = Network::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_bidirectional(station_a, station_b, 60);

        let mut train = Train::new(TrainId(0), 100, station_a, Direction::Forward);

        train.state = TrainState::Moving {
            from: station_a,
            to: station_b,
            elapsed_seconds: 1,
        };

        let mut simulation = Simulation::new(network, vec![train]);

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: station_a,
                to: station_b,
                elapsed_seconds: 2,
            }
        );
    }
}
