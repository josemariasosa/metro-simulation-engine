use crate::dwell::DwellPolicy;
use crate::network::Network;
use crate::snapshot::{SimulationSnapshot, TrainSnapshot, TrainSnapshotState};
use crate::station::StationId;
use crate::train::{AtStationState, Train, TrainState};

#[derive(Debug)]
pub struct Simulation {
    pub elapsed_seconds: u64,
    pub network: Network,
    trains: Vec<Train>,
    dwell_policy: DwellPolicy,
}

fn dwelling_snapshot_state(
    station: StationId,
    elapsed_seconds: u64,
    dwell_seconds: u64,
) -> TrainSnapshotState {
    assert!(
        elapsed_seconds < dwell_seconds,
        "dwelling elapsed time must be less than dwell duration"
    );

    TrainSnapshotState::Dwelling {
        station,
        remaining_seconds: dwell_seconds - elapsed_seconds,
    }
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

    /// Returns an owned observation of the current state in train vector order.
    pub fn snapshot(&self) -> SimulationSnapshot {
        SimulationSnapshot {
            elapsed_seconds: self.elapsed_seconds,
            trains: self
                .trains
                .iter()
                .map(|train| self.snapshot_train(train))
                .collect(),
        }
    }

    fn snapshot_train(&self, train: &Train) -> TrainSnapshot {
        TrainSnapshot {
            id: train.id,
            direction: train.direction,
            state: self.snapshot_train_state(&train.state),
        }
    }

    fn snapshot_train_state(&self, state: &TrainState) -> TrainSnapshotState {
        match state {
            TrainState::AtStation {
                station,
                state:
                    AtStationState::Dwelling {
                        elapsed_seconds,
                        dwell_seconds,
                    },
            } => dwelling_snapshot_state(*station, *elapsed_seconds, *dwell_seconds),
            TrainState::Moving {
                from,
                to,
                elapsed_seconds,
            } => {
                let track = self
                    .network
                    .track(*from, *to)
                    .expect("moving train must reference an existing track");

                TrainSnapshotState::Moving {
                    from: *from,
                    to: *to,
                    elapsed_seconds: *elapsed_seconds,
                    travel_seconds: track.travel_seconds,
                }
            }
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
    use crate::train::{Direction, Train, TrainId, TrainState};

    use super::*;

    /// A fresh bidirectional line for each test. Station IDs follow the supplied order.
    struct SimulationFixture<const N: usize> {
        network: Network,
        stations: [StationId; N],
        dwell_policy: DwellPolicy,
    }

    impl<const N: usize> SimulationFixture<N> {
        fn new(station_names: [&str; N], travel_seconds: u64) -> Self {
            let mut network = Network::new();
            let stations = station_names.map(|name| network.add_station(name));
            for pair in stations.windows(2) {
                network.connect_bidirectional(pair[0], pair[1], travel_seconds);
            }

            Self {
                network,
                stations,
                dwell_policy: DwellPolicy::new(),
            }
        }
    }

    /// Default scenario: one train dwelling at A, with a ten-second track to B.
    fn test_simulation() -> Simulation {
        let SimulationFixture {
            network,
            stations: [a, _],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);
        let train = Train::new(TrainId(0), 100, a, Direction::Forward);
        Simulation::new(network, vec![train], dwell_policy)
    }

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
        let SimulationFixture {
            network,
            stations: [station_a, _],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 60);

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
        let SimulationFixture {
            network,
            stations: [station_a, station_b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);

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
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);

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
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 3);

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
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 1);

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
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 3);

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
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 3);

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
        let SimulationFixture {
            network,
            stations: [_, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 10);

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
        let SimulationFixture {
            network,
            stations: [a, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);

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

    #[test]
    fn initial_dwelling_state_maps_to_snapshot_state() {
        let simulation = test_simulation();
        let train = &simulation.trains()[0];

        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to start dwelling");
        };

        let snapshot_state = dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds);

        assert_eq!(
            snapshot_state,
            TrainSnapshotState::Dwelling {
                station: StationId(0),
                remaining_seconds: 3,
            }
        );
    }

    #[test]
    fn dwelling_snapshot_countdown_matches_future_steps_until_departure() {
        let mut simulation = test_simulation();

        let train = &simulation.trains()[0];

        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to start dwelling");
        };

        assert_eq!(
            dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds,),
            TrainSnapshotState::Dwelling {
                station,
                remaining_seconds: 3,
            }
        );

        simulation.step();

        let train = &simulation.trains()[0];
        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to still be dwelling");
        };

        assert_eq!(
            dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds,),
            TrainSnapshotState::Dwelling {
                station,
                remaining_seconds: 2,
            }
        );

        simulation.step();

        let train = &simulation.trains()[0];
        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to still be dwelling");
        };

        assert_eq!(
            dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds,),
            TrainSnapshotState::Dwelling {
                station,
                remaining_seconds: 1,
            }
        );

        simulation.step();

        assert!(matches!(
            simulation.trains()[0].state,
            TrainState::Moving { .. }
        ));
    }

    #[test]
    fn dwelling_snapshot_uses_stored_dwell_duration() {
        let snapshot = dwelling_snapshot_state(StationId(0), 2, 5);

        assert_eq!(
            snapshot,
            TrainSnapshotState::Dwelling {
                station: StationId(0),
                remaining_seconds: 3,
            }
        );
    }
}
