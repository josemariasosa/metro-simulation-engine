use crate::network::Network;
use crate::train::{Train, TrainState};

#[derive(Debug)]
pub struct Simulation {
    pub elapsed_seconds: u64,
    pub network: Network,
    trains: Vec<Train>,
}

impl Simulation {
    pub fn new(network: Network, trains: Vec<Train>) -> Self {
        Self {
            elapsed_seconds: 0,
            network,
            trains,
        }
    }

    pub fn step(&mut self, dt: u64) {
        self.elapsed_seconds += dt;

        for train in &mut self.trains {
            if let TrainState::AtStation { station } = train.state {
                let Some(next_station) = self.network.next_station(station, train.direction) else {
                    continue;
                };

                if self.network.travel_time(station, next_station).is_some() {
                    train.state = TrainState::Moving {
                        from: station,
                        to: next_station,
                        elapsed_seconds: dt,
                    };
                }
            }
        }
    }

    pub fn trains(&self) -> &Vec<Train> {
        &self.trains
    }
}

#[cfg(test)]
mod tests {
    use crate::train::{Direction, Train, TrainId, TrainState};

    use super::*;

    #[test]
    fn simulation_advances_time() {
        let network = Network::new();
        let trains = Vec::new();

        let mut simulation = Simulation::new(network, trains);

        simulation.step(4);
        simulation.step(5);
        simulation.step(6);

        assert_eq!(simulation.elapsed_seconds, 15);
    }

    #[test]
    fn step_starts_train_moving_toward_next_station() {
        let mut network = Network::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_stations(station_a, station_b, 60);

        let train = Train::new(TrainId(0), 100, station_a, Direction::Forward);

        let mut simulation = Simulation::new(network, vec![train]);

        simulation.step(1);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: station_a,
                to: station_b,
                elapsed_seconds: 1,
            }
        );
    }
}
