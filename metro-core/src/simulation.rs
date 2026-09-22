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

    pub fn step(&mut self) {
        self.elapsed_seconds += 1;

        for train in &mut self.trains {
            match train.state {
                TrainState::AtStation { station } => {
                    match self.network.next_track(station, train.direction) {
                        Some(next_track) => {
                            train.state = TrainState::Moving {
                                from: station,
                                to: next_track.to,
                                elapsed_seconds: 1,
                            };
                        }
                        None => {
                            let next_track = self
                                .network
                                .next_track(station, train.direction.reverse())
                                .expect("NO_NEXT_TRACK_AFTER_REVERSING_DIRECTION");
                            train.state = TrainState::Moving {
                                from: station,
                                to: next_track.to,
                                elapsed_seconds: 1,
                            };
                            train.direction = train.direction.reverse();
                        }
                    }
                }

                TrainState::Moving {
                    from,
                    to,
                    elapsed_seconds,
                } => {
                    if let Some(track) = self.network.track(from, to) {
                        if elapsed_seconds + 1 >= track.travel_seconds {
                            train.state = TrainState::AtStation { station: to };
                        } else {
                            train.state = TrainState::Moving {
                                from,
                                to,
                                elapsed_seconds: elapsed_seconds + 1,
                            };
                        }
                    }
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

        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.elapsed_seconds, 3);
    }

    #[test]
    fn step_starts_train_moving_toward_next_station() {
        let mut network = Network::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_bidirectional(station_a, station_b, 60);

        let train = Train::new(TrainId(0), 100, station_a, Direction::Forward);

        let mut simulation = Simulation::new(network, vec![train]);

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: station_a,
                to: station_b,
                elapsed_seconds: 1,
            }
        );
    }

    #[test]
    fn train_remains_moving_until_track_travel_time_is_reached() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 3);

        let train = Train::new(TrainId(0), 100, a, Direction::Forward);
        let mut simulation = Simulation::new(network, vec![train]);

        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 2,
            }
        );
    }

    #[test]
    fn train_arrives_when_track_travel_time_is_reached() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 3);

        let train = Train::new(TrainId(0), 100, a, Direction::Forward);
        let mut simulation = Simulation::new(network, vec![train]);

        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation { station: b }
        );
    }

    #[test]
    fn train_reverses_direction_at_end_of_line() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");

        network.connect_bidirectional(a, b, 10);
        network.connect_bidirectional(b, c, 10);

        let train = Train::new(TrainId(0), 100, c, Direction::Forward);
        let mut simulation = Simulation::new(network, vec![train]);

        simulation.step();

        assert_eq!(simulation.trains()[0].direction, Direction::Backward);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: c,
                to: b,
                elapsed_seconds: 1,
            }
        );
    }

    #[test]
    fn train_continues_in_reversed_direction_after_reaching_endpoint() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");

        network.connect_bidirectional(a, b, 2);
        network.connect_bidirectional(b, c, 2);

        let train = Train::new(TrainId(0), 100, c, Direction::Forward);
        let mut simulation = Simulation::new(network, vec![train]);

        // C -> B
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation { station: b }
        );

        // It should continue B -> A, still traveling backward.
        simulation.step();

        assert_eq!(simulation.trains()[0].direction, Direction::Backward);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: b,
                to: a,
                elapsed_seconds: 1,
            }
        );
    }
}
