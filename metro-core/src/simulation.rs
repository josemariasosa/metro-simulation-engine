use crate::dwell::DwellPolicy;
use crate::network::Network;
use crate::station::StationId;
use crate::train::{AtStationState, Train, TrainState};

#[derive(Debug)]
pub struct Simulation {
    pub elapsed_seconds: u64,
    pub network: Network,
    trains: Vec<Train>,
    dwell_policy: DwellPolicy,
}

impl Simulation {
    pub fn new(network: Network, trains: Vec<Train>, dwell_policy: DwellPolicy) -> Self {
        Self {
            elapsed_seconds: 0,
            network,
            trains,
            dwell_policy,
        }
    }

    fn step_at_station(
        network: &Network,
        train: &mut Train,
        station: StationId,
        state: AtStationState,
    ) {
        match state {
            AtStationState::Dwelling {
                elapsed_seconds,
                dwell_seconds,
            } => {
                if elapsed_seconds + 1 < dwell_seconds {
                    train.state = TrainState::AtStation {
                        station,
                        state: AtStationState::Dwelling {
                            elapsed_seconds: elapsed_seconds + 1,
                            dwell_seconds,
                        },
                    };
                } else {
                    match network.next_track(station, train.direction) {
                        Some(next_track) => {
                            train.state = TrainState::Moving {
                                from: station,
                                to: next_track.to,
                                elapsed_seconds: 0,
                            };
                        }
                        None => {
                            let next_track = network
                                .next_track(station, train.direction.reverse())
                                .expect("NO_NEXT_TRACK_AFTER_REVERSING_DIRECTION");
                            train.state = TrainState::Moving {
                                from: station,
                                to: next_track.to,
                                elapsed_seconds: 0,
                            };
                            train.direction = train.direction.reverse();
                        }
                    }
                }
            }
        }
    }

    fn step_moving(
        network: &Network,
        dwell_policy: &DwellPolicy,
        train: &mut Train,
        from: StationId,
        to: StationId,
        elapsed_seconds: u64,
    ) {
        let track = network.track(from, to).expect("TRACK_NOT_FOUND");
        if elapsed_seconds + 1 >= track.travel_seconds {
            let dwell_seconds = dwell_policy.dwell_seconds(to, train);
            train.state = TrainState::AtStation {
                station: to,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds,
                },
            };
        } else {
            train.state = TrainState::Moving {
                from,
                to,
                elapsed_seconds: elapsed_seconds + 1,
            };
        }
    }

    pub fn step(&mut self) {
        self.elapsed_seconds += 1;

        for train in &mut self.trains {
            match train.state {
                TrainState::AtStation { station, state } => {
                    Self::step_at_station(&self.network, train, station, state);
                }

                TrainState::Moving {
                    from,
                    to,
                    elapsed_seconds,
                } => {
                    Self::step_moving(
                        &self.network,
                        &self.dwell_policy,
                        train,
                        from,
                        to,
                        elapsed_seconds,
                    );
                }
            }
        }
    }

    pub fn trains(&self) -> &[Train] {
        &self.trains
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        train::{Direction, Train, TrainId, TrainState},
    };

    use super::*;

    #[test]
    fn simulation_advances_time() {
        let network = Network::new();
        let dwell_policy = DwellPolicy::new();
        let trains = Vec::new();

        let mut simulation = Simulation::new(network, trains, dwell_policy);

        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.elapsed_seconds, 3);
    }

    #[test]
    fn new_train_starts_dwelling_and_advances_dwell_time() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_bidirectional(station_a, station_b, 60);

        let train = Train::new(TrainId(0), 100, station_a, Direction::Forward);

        let mut simulation = Simulation::new(network, vec![train.clone()], dwell_policy.clone());

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: station_a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
                }
            }
        );

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: station_a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 1,
                    dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
                }
            }
        );
    }

    #[test]
    fn train_remains_dwelling_until_dwell_time_is_reached() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_bidirectional(station_a, station_b, 10);

        let train = Train::new(TrainId(0), 100, station_a, Direction::Forward);
        let mut simulation = Simulation::new(network, vec![train.clone()], dwell_policy.clone());

        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: station_a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 2,
                    dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
                },
            }
        );

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: station_a,
                to: station_b,
                elapsed_seconds: 0,
            }
        );
    }

    #[test]
    fn step_at_station_preserves_configured_dwell_duration() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 10);

        let mut train = Train::new(TrainId(0), 100, a, Direction::Forward);
        train.state = TrainState::AtStation {
            station: a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 5,
            },
        };

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        simulation.step();
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 4,
                    dwell_seconds: 5,
                },
            }
        );

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );
    }

    #[test]
    fn moving_train_advances_from_zero_elapsed_seconds() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 3);

        let mut train = Train::new(TrainId(0), 100, a, Direction::Forward);
        train.state = TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0,
        };

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 1,
            }
        );
    }

    #[test]
    fn train_arrives_after_one_second_on_one_second_track() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 1);

        let mut train = Train::new(TrainId(0), 100, a, Direction::Forward);
        train.state = TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0,
        };
        let expected_dwell_seconds = dwell_policy.dwell_seconds(b, &train);

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: b,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: expected_dwell_seconds,
                },
            }
        );
    }

    #[test]
    fn train_remains_moving_until_track_travel_time_is_reached() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 3);

        let train = Train::new(TrainId(0), 100, a, Direction::Forward);
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at A for 3 seconds.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );

        // Travel for 2 of the required 3 seconds.
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
        let dwell_policy = DwellPolicy::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 3);

        let train = Train::new(TrainId(0), 100, a, Direction::Forward);
        let expected_dwell_seconds = dwell_policy.dwell_seconds(b, &train);
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at A for 3 seconds.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );

        // Travel from A to B for 3 seconds.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: b,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: expected_dwell_seconds,
                },
            }
        );
    }

    #[test]
    fn train_reverses_direction_at_end_of_line() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");

        network.connect_bidirectional(a, b, 10);
        network.connect_bidirectional(b, c, 10);

        let train = Train::new(TrainId(0), 100, c, Direction::Forward);
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at C for 3 seconds.
        simulation.step();
        simulation.step();
        simulation.step();

        // No track exists forward from C, so the train reverses
        // and starts moving toward B.
        assert_eq!(simulation.trains()[0].direction, Direction::Backward);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: c,
                to: b,
                elapsed_seconds: 0,
            }
        );
    }

    #[test]
    fn train_continues_in_reversed_direction_after_reaching_endpoint() {
        let mut network = Network::new();
        let dwell_policy = DwellPolicy::new();

        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");

        network.connect_bidirectional(a, b, 2);
        network.connect_bidirectional(b, c, 2);

        let train = Train::new(TrainId(0), 100, c, Direction::Forward);
        let expected_dwell_seconds = dwell_policy.dwell_seconds(b, &train);
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at C, then reverse and depart toward B.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.trains()[0].direction, Direction::Backward);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: c,
                to: b,
                elapsed_seconds: 0,
            }
        );

        // Travel C -> B.
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: b,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: expected_dwell_seconds,
                },
            }
        );

        // Dwell at B, then continue toward A.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.trains()[0].direction, Direction::Backward);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: b,
                to: a,
                elapsed_seconds: 0,
            }
        );
    }
}
