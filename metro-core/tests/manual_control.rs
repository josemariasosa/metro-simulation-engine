use metro_core::DwellPolicy;
use metro_core::Network;
use metro_core::StationId;
use metro_core::command::{CommandError, TrainCommand};
use metro_core::simulation::Simulation;
use metro_core::snapshot::TrainSnapshotState;
use metro_core::{AtStationState, Direction, Train, TrainId, TrainState};

// fn manual_a_b_simulation() -> Simulation {
//     manual_a_b_simulation_with_travel_seconds(10)
// }

// fn manual_a_b_simulation_with_travel_seconds(travel_seconds: u64) -> Simulation {
//     let mut network = Network::new();
//     let a = network.add_station("A");
//     let b = network.add_station("B");
//     network.connect_bidirectional(a, b, travel_seconds);
//     let train = Train::new_manual(100, a, Direction::Forward, 3);
//     Simulation::new(network, vec![train], DwellPolicy::new())
// }

#[test]
fn manual_train_arrival_resets_velocity_and_starts_fresh_dwell() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 2);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();
    assert_eq!(
        Simulation::snapshot_train_by_id(&simulation.snapshot(), train_id).state,
        TrainSnapshotState::Dwelling {
            station: StationId(0),
            remaining_seconds: 2,
        }
    );

    // Abandon a partially consumed dwell before its normal completion.
    simulation
        .apply_command(TrainCommand::Accelerate {
            train_id: TrainId(0),
        })
        .unwrap();
    simulation.step();
    let moving = simulation.snapshot();
    assert_eq!(
        Simulation::snapshot_train_by_id(&moving, train_id).velocity,
        1
    );
    assert_eq!(
        moving.trains[0].state,
        TrainSnapshotState::Moving {
            from: StationId(0),
            to: StationId(1),
            elapsed_seconds: 1,
            travel_seconds: 2,
        }
    );
    simulation.step();

    let snapshot = simulation.snapshot();
    assert_eq!(snapshot.elapsed_seconds, 3);
    assert_eq!(
        Simulation::snapshot_train_by_id(&snapshot, train_id).velocity,
        0
    );
    assert_eq!(
        Simulation::snapshot_train_by_id(&snapshot, train_id).state,
        TrainSnapshotState::Dwelling {
            station: StationId(1),
            remaining_seconds: 3,
        }
    );
    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::AtStation {
            station: StationId(1),
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3,
            },
        }
    );
}

#[test]
fn manual_train_can_accelerate_immediately_after_arrival() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 2);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    simulation
        .apply_command(TrainCommand::Accelerate { train_id })
        .unwrap();
    simulation.step();
    simulation.step();
    let arrival = simulation.snapshot();
    assert_eq!(
        Simulation::snapshot_train_by_id(&arrival, train_id).velocity,
        0
    );
    assert_eq!(
        Simulation::snapshot_train_by_id(&arrival, train_id).state,
        TrainSnapshotState::Dwelling {
            station: StationId(1),
            remaining_seconds: 3,
        }
    );

    simulation
        .apply_command(TrainCommand::Accelerate { train_id })
        .unwrap();

    let snapshot = simulation.snapshot();
    assert_eq!(snapshot.elapsed_seconds, arrival.elapsed_seconds);
    assert_eq!(
        Simulation::snapshot_train_by_id(&snapshot, train_id).velocity,
        1
    );
    assert_eq!(
        Simulation::snapshot_train_by_id(&snapshot, train_id).direction,
        Direction::Backward
    );
    assert_eq!(
        Simulation::snapshot_train_by_id(&snapshot, train_id).state,
        TrainSnapshotState::Moving {
            from: StationId(1),
            to: StationId(0),
            elapsed_seconds: 0,
            travel_seconds: 2,
        }
    );
}

#[test]
fn one_second_track_arrival_starts_dwell_at_zero_elapsed() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 1);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    simulation
        .apply_command(TrainCommand::Accelerate { train_id })
        .unwrap();
    simulation.step();

    let snapshot = simulation.snapshot();
    assert_eq!(snapshot.elapsed_seconds, 1);
    assert_eq!(
        Simulation::snapshot_train_by_id(&snapshot, train_id).velocity,
        0
    );
    assert_eq!(
        Simulation::snapshot_train_by_id(&snapshot, train_id).state,
        TrainSnapshotState::Dwelling {
            station: StationId(1),
            remaining_seconds: 3,
        }
    );
    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::AtStation {
            station: StationId(1),
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3,
            },
        }
    );
}

#[test]
fn accelerate_manual_train_from_initial_dwell() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 10);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    let before_time = simulation.elapsed_seconds;
    let initial_snapshot = simulation.snapshot();

    assert_eq!(before_time, 0);
    assert_eq!(
        Simulation::snapshot_train_by_id(&initial_snapshot, train_id).velocity,
        0
    );
    assert_eq!(
        Simulation::snapshot_train_by_id(&initial_snapshot, train_id).state,
        TrainSnapshotState::Dwelling {
            station: StationId(0),
            remaining_seconds: 3,
        }
    );

    // Departure is allowed before dwell completes; core chooses the destination.
    let result = simulation.apply_command(TrainCommand::Accelerate {
        train_id: TrainId(0),
    });

    assert_eq!(result, Ok(()));
    assert_eq!(simulation.elapsed_seconds, before_time);

    let snapshot = simulation.snapshot();
    let train = &Simulation::snapshot_train_by_id(&snapshot, train_id);

    assert_eq!(train.velocity, 1);
    assert_eq!(train.direction, Direction::Forward);
    assert!(matches!(
        train.state,
        TrainSnapshotState::Moving {
            from,
            to,
            elapsed_seconds: 0,
            ..
        } if from == StationId(0) && to == StationId(1)
    ));
}

#[test]
fn accelerate_manual_train_from_ready() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 10);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    for _ in 0..3 {
        simulation.step();
    }

    let before_time = simulation.elapsed_seconds;
    let ready_snapshot = simulation.snapshot();

    assert_eq!(before_time, 3);
    assert_eq!(
        Simulation::snapshot_train_by_id(&ready_snapshot, train_id).velocity,
        0
    );
    assert_eq!(
        Simulation::snapshot_train_by_id(&ready_snapshot, train_id).state,
        TrainSnapshotState::Ready {
            station: StationId(0),
        }
    );

    let result = simulation.apply_command(TrainCommand::Accelerate {
        train_id: TrainId(0),
    });

    assert_eq!(result, Ok(()));
    assert_eq!(simulation.elapsed_seconds, before_time);

    let snapshot = simulation.snapshot();
    let train = Simulation::snapshot_train_by_id(&snapshot, train_id);

    assert_eq!(train.velocity, 1);
    assert_eq!(train.direction, Direction::Forward);
    assert!(matches!(
        train.state,
        TrainSnapshotState::Moving {
            from,
            to,
            elapsed_seconds: 0,
            ..
        } if from == StationId(0) && to == StationId(1)
    ));
}

#[test]
fn accelerate_while_manual_train_is_moving_is_idempotent() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 10);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let other_train = Train::new_manual(100, b, Direction::Backward, 3);
    let mut simulation = Simulation::new(network, vec![train, other_train], DwellPolicy::new());

    simulation
        .apply_command(TrainCommand::Accelerate {
            train_id: TrainId(0),
        })
        .unwrap();
    simulation.step();

    let before = simulation.snapshot();
    assert_eq!(before.elapsed_seconds, 1);
    assert_eq!(before.trains[0].velocity, 1);
    assert_eq!(before.trains[0].direction, Direction::Forward);
    assert!(matches!(
        before.trains[0].state,
        TrainSnapshotState::Moving {
            from,
            to,
            elapsed_seconds: 1,
            ..
        } if from == a && to == b
    ));

    let result = simulation.apply_command(TrainCommand::Accelerate {
        train_id: TrainId(0),
    });
    let after = simulation.snapshot();

    assert_eq!(result, Ok(()));
    // Includes the simulation clock and every train's state, direction and velocity.
    assert_eq!(after, before);
}

#[test]
fn manual_train_becomes_ready_after_dwell_completes() {
    let mut network = Network::new();
    let station_a = network.add_station("A");

    let train = Train::new_manual(100, station_a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    for elapsed_seconds in 0..3 {
        assert_eq!(simulation.elapsed_seconds, elapsed_seconds);
        assert_eq!(
            simulation.train(train_id).state(),
            TrainState::AtStation {
                station: station_a,
                state: AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds: 3,
                },
            }
        );

        simulation.step();
    }

    assert_eq!(simulation.elapsed_seconds, 3);
    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::AtStation {
            station: station_a,
            state: AtStationState::Ready,
        }
    );
    assert_eq!(simulation.snapshot().trains[0].velocity, 0);
}

#[test]
fn manual_train_remains_ready_without_input() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 10);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();
    simulation.step();
    simulation.step();

    let ready_state = simulation.train(train_id).state();
    let direction = simulation.train(train_id).direction();
    assert_eq!(simulation.elapsed_seconds, 3);
    assert_eq!(
        ready_state,
        TrainState::AtStation {
            station: a,
            state: AtStationState::Ready,
        }
    );
    assert_eq!(simulation.snapshot().trains[0].velocity, 0);

    for elapsed_seconds in 4..=5 {
        simulation.step();

        assert_eq!(simulation.elapsed_seconds, elapsed_seconds);
        assert_eq!(simulation.train(train_id).state(), ready_state);
        assert_eq!(simulation.train(train_id).direction(), direction);
        assert_eq!(simulation.snapshot().trains[0].velocity, 0);
    }
}

#[test]
fn manual_train_waits_ready_at_endpoint_without_reversing() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 10);
    let train = Train::new_manual(100, b, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    for _ in 0..3 {
        simulation.step();
    }
    let ready_snapshot = simulation.snapshot();
    assert_eq!(
        ready_snapshot.trains[0].state,
        TrainSnapshotState::Ready { station: b }
    );

    for elapsed_seconds in 4..=10 {
        simulation.step();
        assert_eq!(simulation.elapsed_seconds, elapsed_seconds);
        assert_eq!(simulation.train(train_id).direction(), Direction::Forward);
        assert_eq!(
            simulation.train(train_id).state(),
            TrainState::AtStation {
                station: b,
                state: AtStationState::Ready,
            }
        );
        assert_eq!(simulation.snapshot().trains[0].velocity, 0);
        assert_eq!(simulation.snapshot().trains, ready_snapshot.trains);
    }
    assert_eq!(ready_snapshot.elapsed_seconds, 3);
}

// Compare domain timers as well as the public observation for every train.
fn assert_command_unchanged(
    simulation: &mut Simulation,
    train_id: TrainId,
    expected: Result<(), CommandError>,
) {
    let before = simulation.snapshot();
    let domain_before: Vec<_> = simulation
        .trains()
        .map(|(id, t)| (id, t.capacity(), t.state(), t.direction()))
        .collect();
    assert_eq!(
        simulation.apply_command(TrainCommand::Accelerate { train_id }),
        expected
    );
    assert_eq!(simulation.snapshot(), before);
    let domain_after: Vec<_> = simulation
        .trains()
        .map(|(id, t)| (id, t.capacity(), t.state(), t.direction()))
        .collect();
    assert_eq!(domain_after, domain_before);
}

#[test]
fn blocked_manual_intent_is_not_buffered_when_destination_becomes_available() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");
    network.connect_bidirectional(a, b, 2);
    network.connect_bidirectional(b, c, 2);
    let trains = vec![
        Train::new_manual(100, a, Direction::Forward, 3),
        Train::new_manual(100, b, Direction::Forward, 3),
    ];
    let mut simulation = Simulation::new(network, trains, DwellPolicy::new());
    assert_command_unchanged(&mut simulation, TrainId(0), Err(CommandError::Blocked));
    simulation
        .apply_command(TrainCommand::Accelerate {
            train_id: TrainId(1),
        })
        .unwrap();
    assert_eq!(simulation.elapsed_seconds, 0);

    for elapsed in 1..=3 {
        simulation.step();
        assert_eq!(simulation.elapsed_seconds, elapsed);
        let snapshot = simulation.snapshot();
        let train = snapshot
            .trains
            .iter()
            .find(|train| train.id == TrainId(0))
            .unwrap();
        assert_eq!(train.direction, Direction::Forward);
        assert_eq!(train.velocity, 0);
        assert_eq!(
            train.state,
            if elapsed < 3 {
                TrainSnapshotState::Dwelling {
                    station: a,
                    remaining_seconds: 3 - elapsed,
                }
            } else {
                TrainSnapshotState::Ready { station: a }
            }
        );
    }

    simulation
        .apply_command(TrainCommand::Accelerate {
            train_id: TrainId(0),
        })
        .unwrap();
    let snapshot = simulation.snapshot();
    assert_eq!(snapshot.elapsed_seconds, 3);
    let train = snapshot
        .trains
        .iter()
        .find(|train| train.id == TrainId(0))
        .unwrap();
    assert_eq!(train.direction, Direction::Forward);
    assert_eq!(train.velocity, 1);
    assert_eq!(
        train.state,
        TrainSnapshotState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0,
            travel_seconds: 2,
        }
    );
}

#[test]
fn command_rejections_preserve_all_trains_and_timers() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let isolated = network.add_station("Isolated");
    let unrelated = network.add_station("Unrelated");
    network.connect_bidirectional(a, b, 10);
    let trains = vec![
        Train::new(100, a, Direction::Forward, 3),
        Train::new_manual(100, isolated, Direction::Forward, 3),
        Train::new_manual(100, unrelated, Direction::Forward, 3),
    ];
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id_1 = simulation.add_train(trains[0].clone()).unwrap();
    let train_id_2 = simulation.add_train(trains[1].clone()).unwrap();
    let train_id_3 = simulation.add_train(trains[2].clone()).unwrap();

    for steps in 0..=3 {
        // An unallocated identity is unknown.
        assert_command_unchanged(&mut simulation, train_id_3, Err(CommandError::UnknownTrain));
        assert_command_unchanged(
            &mut simulation,
            TrainId(999),
            Err(CommandError::UnknownTrain),
        );
        assert_command_unchanged(&mut simulation, train_id_1, Err(CommandError::NotManual));
        assert_command_unchanged(
            &mut simulation,
            TrainId(1),
            Err(CommandError::NoOutgoingTrack),
        );
        if steps < 3 {
            simulation.step();
        }
    }
    assert!(matches!(
        simulation.train(train_id_1).state(),
        TrainState::Moving { .. }
    ));
    assert!(matches!(
        simulation.train(train_id_2).state(),
        TrainState::AtStation {
            state: AtStationState::Ready,
            ..
        }
    ));
}

#[test]
fn duplicate_acceleration_preserves_zero_and_advanced_traversal() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 10);
    let train = Train::new_manual(100, b, Direction::Backward, 3);
    let other = Train::new(200, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let _ = simulation.add_train(other).unwrap();
    let train_id = simulation.add_train(train).unwrap();

    simulation
        .apply_command(TrainCommand::Accelerate { train_id })
        .unwrap();

    for elapsed_seconds in 0..=1 {
        assert_eq!(simulation.elapsed_seconds, elapsed_seconds);
        assert_eq!(
            simulation.train(train_id).state(),
            TrainState::Moving {
                from: b,
                to: a,
                elapsed_seconds,
            }
        );
        assert_eq!(simulation.train(train_id).direction(), Direction::Backward);
        assert_eq!(
            Simulation::snapshot_train_by_id(&simulation.snapshot(), train_id).velocity,
            1
        );
        // Include the unrelated train's raw dwell timers, which snapshots collapse
        // to remaining_seconds, at both zero and nonzero traversal time.
        assert_command_unchanged(&mut simulation, TrainId(1), Ok(()));
        if elapsed_seconds == 0 {
            simulation.step();
        }
    }
}

#[test]
fn non_manual_validation_precedes_departure_selection() {
    let mut network = Network::new();
    let station = network.add_station("Isolated");
    let train = Train::new(100, station, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());
    assert_command_unchanged(&mut simulation, TrainId(0), Err(CommandError::NotManual));
}

#[test]
fn acceleration_selects_current_direction_or_reverse_fallback() {
    // Exercise both directions, endpoints, and a missing forward track at B.
    for (station, direction, gap, expected_to, expected_direction) in [
        (1, Direction::Forward, false, 2, Direction::Forward),
        (1, Direction::Backward, false, 0, Direction::Backward),
        (2, Direction::Forward, false, 1, Direction::Backward),
        (0, Direction::Backward, false, 1, Direction::Forward),
        (1, Direction::Forward, true, 0, Direction::Backward),
    ] {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");

        network.connect_bidirectional(a, b, 10);

        if !gap {
            network.connect_bidirectional(b, c, 10);
        }

        let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());

        let other_id = simulation
            .add_train(Train::new_manual(100, a, Direction::Forward, 3))
            .unwrap();

        let train_id = simulation
            .add_train(Train::new_manual(100, StationId(station), direction, 3))
            .unwrap();

        simulation.step();

        assert_eq!(
            simulation.train(train_id).state(),
            TrainState::AtStation {
                station: StationId(station),
                state: AtStationState::Dwelling {
                    elapsed_seconds: 1,
                    dwell_seconds: 3,
                },
            }
        );

        let before = simulation.snapshot();

        simulation
            .apply_command(TrainCommand::Accelerate { train_id })
            .unwrap();

        let after = simulation.snapshot();

        // Commands do not advance simulation time.
        assert_eq!(after.elapsed_seconds, before.elapsed_seconds);

        // The unrelated train is untouched.
        assert_eq!(
            Simulation::snapshot_train_by_id(&after, other_id),
            Simulation::snapshot_train_by_id(&before, other_id),
        );

        // Departure selects the current direction when available,
        // otherwise the reverse fallback.
        let train = simulation.train(train_id);

        assert_eq!(train.direction(), expected_direction);
        assert_eq!(train.velocity(), 1);

        // The unfinished dwell is replaced without consuming
        // a traversal second.
        assert_eq!(
            train.state(),
            TrainState::Moving {
                from: StationId(station),
                to: StationId(expected_to),
                elapsed_seconds: 0,
            }
        );

        let snapshot_train = Simulation::snapshot_train_by_id(&after, train_id);

        assert_eq!(snapshot_train.direction, expected_direction);
        assert_eq!(snapshot_train.velocity, 1);

        assert_eq!(
            snapshot_train.state,
            TrainSnapshotState::Moving {
                from: StationId(station),
                to: StationId(expected_to),
                elapsed_seconds: 0,
                travel_seconds: 10,
            }
        );
    }
}

#[test]
fn manual_round_trip_matches_spec_002_vertical_slice() {
    use Direction::{Backward, Forward};
    use TrainSnapshotState::{Dwelling, Moving, Ready};
    let a = StationId(0);
    let b = StationId(1);
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 2);
    let train = Train::new_manual(100, a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let train_id = simulation.add_train(train).unwrap();

    let assert_train = |simulation: &Simulation, time, direction, velocity, state| {
        let snapshot = simulation.snapshot();
        assert_eq!(snapshot.elapsed_seconds, time);
        assert_eq!(snapshot.trains[0].direction, direction);
        assert_eq!(snapshot.trains[0].velocity, velocity);
        assert_eq!(snapshot.trains[0].state, state);
    };
    let dwelling = |station, remaining_seconds| Dwelling {
        station,
        remaining_seconds,
    };
    let moving = |from, to, elapsed_seconds| Moving {
        from,
        to,
        elapsed_seconds,
        travel_seconds: 2,
    };
    let accelerate = TrainCommand::Accelerate {
        train_id: TrainId(0),
    };

    assert_train(&simulation, 0, Forward, 0, dwelling(a, 3));
    simulation.step();
    assert_train(&simulation, 1, Forward, 0, dwelling(a, 2));
    assert_eq!(simulation.apply_command(accelerate), Ok(()));
    assert_train(&simulation, 1, Forward, 1, moving(a, b, 0));
    simulation.step();
    assert_train(&simulation, 2, Forward, 1, moving(a, b, 1));
    assert_command_unchanged(&mut simulation, TrainId(0), Ok(()));
    simulation.step();
    assert_train(&simulation, 3, Forward, 0, dwelling(b, 3));
    for _ in 0..3 {
        simulation.step();
    }
    assert_train(&simulation, 6, Forward, 0, Ready { station: b });
    let ready = simulation.train(train_id).state();
    simulation.step();
    assert_train(&simulation, 7, Forward, 0, Ready { station: b });
    assert_eq!(simulation.train(train_id).state(), ready);
    assert_eq!(simulation.apply_command(accelerate), Ok(()));
    assert_train(&simulation, 7, Backward, 1, moving(b, a, 0));
    simulation.step();
    assert_train(&simulation, 8, Backward, 1, moving(b, a, 1));
    simulation.step();
    assert_train(&simulation, 9, Backward, 0, dwelling(a, 3));
    assert_eq!(simulation.apply_command(accelerate), Ok(()));
    assert_train(&simulation, 9, Forward, 1, moving(a, b, 0));
}

// #[test]
// fn ordered_commands_and_steps_are_deterministic_despite_observation_frequency() {
//     let mut frequent = manual_a_b_simulation_with_travel_seconds(2);
//     let mut sparse = manual_a_b_simulation_with_travel_seconds(2);
//     let accelerate = TrainCommand::Accelerate {
//         train_id: TrainId(0),
//     };
//     let unknown = TrainCommand::Accelerate {
//         train_id: TrainId(99),
//     };
//     // None is one complete step; commands commit between those steps.
//     let trace = [
//         None,
//         Some(accelerate),
//         None,
//         Some(accelerate),
//         Some(unknown),
//         None,
//         None,
//         None,
//         None,
//         None,
//         Some(accelerate),
//         None,
//         None,
//         Some(accelerate),
//         None,
//     ];
//     for operation in trace {
//         frequent.snapshot();
//         frequent.snapshot();
//         match operation {
//             Some(command) => {
//                 let expected = if command == unknown {
//                     Err(CommandError::UnknownTrain)
//                 } else {
//                     Ok(())
//                 };
//                 assert_eq!(frequent.apply_command(command), expected);
//                 assert_eq!(sparse.apply_command(command), expected);
//             }
//             None => {
//                 frequent.step();
//                 sparse.step();
//             }
//         }
//         frequent.snapshot();
//         // Compare committed domain state at every boundary without observing sparse.
//         assert_eq!(frequent.elapsed_seconds, sparse.elapsed_seconds);
//         assert_eq!(
//             frequent.trains(train_id).state(),
//             sparse.trains()[0].train.state()
//         );
//         assert_eq!(
//             frequent.trains()[0].train.direction(),
//             sparse.trains()[0].train.direction()
//         );
//     }
//     assert_eq!(frequent.snapshot(), sparse.snapshot());
// }

// #[test]
// fn retained_snapshot_survives_command_at_unchanged_timestamp() {
//     let mut simulation = manual_a_b_simulation();
//     let untouched = manual_a_b_simulation();
//     let retained = simulation.snapshot();
//     let expected = retained.clone();
//     assert_eq!(
//         simulation.apply_command(TrainCommand::Accelerate {
//             train_id: TrainId(0)
//         }),
//         Ok(())
//     );
//     let committed = simulation.snapshot();
//     assert_eq!(retained, expected);
//     assert_eq!(retained, untouched.snapshot());
//     assert_eq!(committed.elapsed_seconds, retained.elapsed_seconds);
//     assert_ne!(committed, untouched.snapshot());
//     assert_eq!(committed.trains[0].velocity, 1);
//     assert_eq!(
//         committed.trains[0].state,
//         TrainSnapshotState::Moving {
//             from: StationId(0),
//             to: StationId(1),
//             elapsed_seconds: 0,
//             travel_seconds: 10,
//         }
//     );
// }

// #[test]
// fn command_before_step_consumes_traversal_but_step_before_command_consumes_dwell() {
//     let mut command_first = manual_a_b_simulation();
//     let mut step_first = manual_a_b_simulation();
//     let accelerate = TrainCommand::Accelerate {
//         train_id: TrainId(0),
//     };
//     assert_eq!(command_first.apply_command(accelerate), Ok(()));
//     command_first.step();
//     step_first.step();
//     assert_eq!(
//         step_first.snapshot().trains[0].state,
//         TrainSnapshotState::Dwelling {
//             station: StationId(0),
//             remaining_seconds: 2,
//         }
//     );
//     assert_eq!(step_first.apply_command(accelerate), Ok(()));
//     for (simulation, traversal_elapsed) in [(&command_first, 1), (&step_first, 0)] {
//         let snapshot = simulation.snapshot();
//         assert_eq!(snapshot.elapsed_seconds, 1);
//         assert_eq!(snapshot.trains[0].velocity, 1);
//         assert_eq!(snapshot.trains[0].direction, Direction::Forward);
//         assert_eq!(
//             snapshot.trains[0].state,
//             TrainSnapshotState::Moving {
//                 from: StationId(0),
//                 to: StationId(1),
//                 elapsed_seconds: traversal_elapsed,
//                 travel_seconds: 10,
//             }
//         );
//     }
// }
