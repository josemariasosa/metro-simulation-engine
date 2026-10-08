use super::*;

#[test]
fn scheduled_admission_obeys_exact_activation_and_expiry() {
    for manual in [false, true] {
        for time in 10..=15 {
            let (mut simulation, train_id) = world(manual, 0, Direction::Forward, 1);

            simulation.elapsed_seconds = 10;

            schedule(&mut simulation, closure(0), 11, Some(15));

            simulation.elapsed_seconds = time;

            let blocked = (11..15).contains(&time);

            assert_eq!(
                simulation
                    .active_constraint_view()
                    .permits_departure(StationId(0), StationId(1),),
                !blocked,
            );

            if manual {
                assert_eq!(
                    command_const(&mut simulation, train_id),
                    if blocked {
                        Err(CommandError::Blocked)
                    } else {
                        Ok(())
                    },
                );
            } else {
                simulation.step();
            }

            assert_eq!(moving_const(&simulation, train_id), !blocked,);

            if time == 10 {
                if manual {
                    simulation.step();
                }

                assert_eq!(simulation.elapsed_seconds, 11);

                assert!(
                    !simulation
                        .active_constraint_view()
                        .permits_departure(StationId(0), StationId(1),)
                );
            }
        }
    }
}

#[test]
fn single_step_constraint_restricts_only_step_beginning_at_start() {
    for time in 10..=12 {
        let (mut simulation, train_id) = world(false, 0, Direction::Forward, 1);

        simulation.elapsed_seconds = 10;
        schedule(&mut simulation, closure(0), 11, Some(12));
        simulation.elapsed_seconds = time;

        simulation.step();

        assert_eq!(moving_const(&simulation, train_id), time != 11);
    }
}

#[test]
fn automatic_ready_train_retries_only_on_later_steps() {
    let (mut simulation, train_id) = world(false, 0, Direction::Forward, 2);

    schedule(&mut simulation, closure(0), 1, Some(4));

    simulation.step();

    for time in 2..=4 {
        simulation.step();

        assert_eq!(simulation.elapsed_seconds, time);

        let train = simulation.train(train_id).unwrap();

        assert_eq!(
            train.state(),
            TrainState::AtStation {
                station: StationId(0),
                state: AtStationState::Ready,
            }
        );

        assert_eq!(train.velocity(), 0);
    }

    assert!(simulation.constraints.is_empty());

    simulation.step();

    assert!(moving_const(&simulation, train_id));
}

#[test]
fn operational_manual_rejection_preserves_entire_world() {
    let (mut simulation, train_id) = world(true, 2, Direction::Forward, 3);

    schedule(&mut simulation, closure(2), 1, None);

    simulation.step();

    assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));
}

#[test]
fn clearing_constraint_does_not_replay_manual_intent() {
    for remove in [false, true] {
        let (mut simulation, train_id) = world(true, 0, Direction::Forward, 1);

        let constraint_id = schedule(&mut simulation, closure(0), 1, Some(3));

        simulation.step();

        assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));

        if remove {
            simulation.remove_constraint(constraint_id).unwrap();
        }

        while simulation.elapsed_seconds < 5 {
            simulation.step();

            assert!(!moving_const(&simulation, train_id));
        }

        assert_eq!(command_const(&mut simulation, train_id), Ok(()));
    }
}

#[test]
fn active_track_closure_is_directed_and_does_not_reroute() {
    let value = OperationalConstraint::TrackUnavailable {
        from: StationId(1),
        to: StationId(2),
    };
    check_action(value, 1, Direction::Forward, false);
    check_action(value, 2, Direction::Backward, true);
}

#[test]
fn departure_block_allows_inbound_and_blocks_terminal_reversal() {
    let value = OperationalConstraint::StationDeparturesBlocked {
        station: StationId(1),
    };
    check_action(value, 1, Direction::Forward, false);
    check_action(value, 0, Direction::Forward, true);
    check_action(
        OperationalConstraint::StationDeparturesBlocked {
            station: StationId(2),
        },
        2,
        Direction::Forward,
        false,
    );
}

#[test]
fn station_unavailable_blocks_both_slots_without_displacing_occupants() {
    for (station, direction) in [
        (0, Direction::Forward),
        (2, Direction::Backward),
        (1, Direction::Forward),
        (1, Direction::Backward),
    ] {
        check_action(closure(1), station, direction, false);
    }
}

#[test]
fn future_constraints_over_claimed_resources_preserve_traversal() {
    let (mut plain, plain_train_id) = world(true, 0, Direction::Forward, 1);

    let (mut constrained, constrained_train_id) = world(true, 0, Direction::Forward, 1);

    command_const(&mut plain, plain_train_id).unwrap();
    command_const(&mut constrained, constrained_train_id).unwrap();

    plain
        .add_train(Train::new_manual(100, StationId(2), Direction::Forward, 1))
        .unwrap();

    constrained
        .add_train(Train::new_manual(100, StationId(2), Direction::Forward, 1))
        .unwrap();

    let physical_before = claims(&constrained);

    for value in [
        OperationalConstraint::TrackUnavailable {
            from: StationId(0),
            to: StationId(1),
        },
        closure(1),
        closure(2),
    ] {
        schedule(&mut constrained, value, 1, None);
    }

    assert_eq!(claims(&constrained), physical_before);

    for _ in 0..5 {
        plain.step();
        constrained.step();

        assert_eq!(constrained.snapshot().trains, plain.snapshot().trains);

        assert_eq!(claims(&constrained), claims(&plain));
    }

    assert!(matches!(
        constrained.train(constrained_train_id).unwrap().state(),
        TrainState::AtStation {
            station: StationId(1),
            ..
        }
    ));

    assert_exact_command(
        &mut constrained,
        constrained_train_id,
        Err(CommandError::Blocked),
    );

    assert_physical_invariants(&constrained);
}

#[test]
fn occupied_station_keeps_dwell_and_blocks_new_departure_until_reopening() {
    let (mut simulation, train_id) = world(false, 1, Direction::Forward, 3);

    schedule(&mut simulation, closure(1), 1, Some(4));

    for expected_time in 1..=4 {
        simulation.step();

        assert_physical_invariants(&simulation);

        assert_eq!(simulation.elapsed_seconds, expected_time);

        assert!(matches!(
            simulation.train(train_id).unwrap().state(),
            TrainState::AtStation {
                station: StationId(1),
                ..
            }
        ));
    }

    assert!(simulation.snapshot().constraints.is_empty());

    simulation.step();

    assert_physical_invariants(&simulation);

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: StationId(1),
            to: StationId(2),
            elapsed_seconds: 0,
        }
    );
}

#[test]
fn moving_manual_accelerate_is_exact_noop_under_active_constraints() {
    let (mut simulation, train_id) = world(true, 0, Direction::Forward, 1);

    schedule(&mut simulation, closure(1), 1, None);

    command_const(&mut simulation, train_id).unwrap();
    simulation.step();

    assert_exact_command(&mut simulation, train_id, Ok(()));
}

#[test]
fn overlapping_causes_block_until_last_cause_clears() {
    for duplicate in [false, true] {
        for remove_first in [false, true] {
            let (mut simulation, train_id) = world(true, 0, Direction::Forward, 1);

            let first = schedule(&mut simulation, closure(0), 1, Some(2));

            let second_value = if duplicate {
                closure(0)
            } else {
                OperationalConstraint::TrackUnavailable {
                    from: StationId(0),
                    to: StationId(1),
                }
            };

            let second = schedule(&mut simulation, second_value, 1, None);

            simulation.step();

            if remove_first {
                simulation.remove_constraint(first).unwrap();
            } else {
                simulation.step();
            }

            assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));

            simulation.remove_constraint(second).unwrap();

            command_const(&mut simulation, train_id).unwrap();
        }
    }
}

#[test]
fn equal_time_handoff_keeps_duplicate_causes_continuously_effective() {
    let (mut simulation, train_id) = world(true, 0, Direction::Forward, 1);

    let ending_at_five = simulation
        .create_constraint_at(closure(0), 1, Some(5), ConstraintOrigin::Planned)
        .unwrap();

    let starting_at_five = simulation
        .create_constraint_at(closure(0), 5, Some(8), ConstraintOrigin::Injected)
        .unwrap();

    let removed_duplicate = simulation
        .create_constraint_at(closure(0), 1, Some(4), ConstraintOrigin::Injected)
        .unwrap();

    let expiring_duplicate = simulation
        .create_constraint_at(closure(0), 1, Some(6), ConstraintOrigin::Planned)
        .unwrap();

    for _ in 0..3 {
        simulation.step();
        assert_physical_invariants(&simulation);
    }

    assert_eq!(simulation.elapsed_seconds, 3);

    assert_eq!(
        simulation
            .snapshot()
            .constraints
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![
            ending_at_five,
            starting_at_five,
            removed_duplicate,
            expiring_duplicate,
        ]
    );

    simulation.remove_constraint(removed_duplicate).unwrap();

    assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));

    // t = 4
    simulation.step();
    assert_physical_invariants(&simulation);

    assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));

    // t = 5: first cause expires exactly as another begins.
    simulation.step();
    assert_physical_invariants(&simulation);

    assert_eq!(simulation.elapsed_seconds, 5);

    assert_eq!(
        simulation
            .snapshot()
            .constraints
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![starting_at_five, expiring_duplicate]
    );

    assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));

    // t = 6: expiring duplicate disappears, handoff constraint remains.
    simulation.step();
    assert_physical_invariants(&simulation);

    assert_eq!(simulation.elapsed_seconds, 6);

    assert_eq!(simulation.snapshot().constraints[0].id, starting_at_five);

    assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));

    // t = 7
    simulation.step();
    assert_physical_invariants(&simulation);

    assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));

    // t = 8: final cause expires.
    simulation.step();
    assert_physical_invariants(&simulation);

    assert_eq!(simulation.elapsed_seconds, 8);
    assert!(simulation.snapshot().constraints.is_empty());

    command_const(&mut simulation, train_id).unwrap();

    assert_physical_invariants(&simulation);
}

#[test]
fn terminal_reversal_retries_after_operational_expiry() {
    let (mut simulation, train_id) = world(false, 2, Direction::Forward, 3);

    schedule(
        &mut simulation,
        OperationalConstraint::TrackUnavailable {
            from: StationId(2),
            to: StationId(1),
        },
        1,
        Some(4),
    );

    for expected_time in 1..=4 {
        simulation.step();

        assert_physical_invariants(&simulation);
        assert_eq!(simulation.elapsed_seconds, expected_time);

        if expected_time < 4 {
            let train = simulation.train(train_id).unwrap();

            assert_eq!(train.direction(), Direction::Forward);

            assert!(matches!(
                train.state(),
                TrainState::AtStation {
                    station: StationId(2),
                    ..
                }
            ));

            if expected_time >= 3 {
                assert!(matches!(
                    train.state(),
                    TrainState::AtStation {
                        state: AtStationState::Ready,
                        ..
                    }
                ));
            }
        }
    }

    assert!(simulation.snapshot().constraints.is_empty());

    let train = simulation.train(train_id).unwrap();

    assert_eq!(train.direction(), Direction::Forward);
    assert_eq!(
        train.state(),
        TrainState::AtStation {
            station: StationId(2),
            state: AtStationState::Ready,
        }
    );

    simulation.step();

    assert_physical_invariants(&simulation);

    let train = simulation.train(train_id).unwrap();

    assert_eq!(train.direction(), Direction::Backward);
    assert_eq!(
        train.state(),
        TrainState::Moving {
            from: StationId(2),
            to: StationId(1),
            elapsed_seconds: 0,
        }
    );
}

#[test]
fn constraint_origin_does_not_change_admission_or_claim_traces() {
    for manual in [false, true] {
        let (mut planned, planned_train_id) = world(manual, 0, Direction::Forward, 2);

        let (mut injected, injected_train_id) = world(manual, 0, Direction::Forward, 2);

        for (simulation, origin) in [
            (&mut planned, ConstraintOrigin::Planned),
            (&mut injected, ConstraintOrigin::Injected),
        ] {
            simulation
                .create_constraint_at(closure(0), 1, Some(4), origin)
                .unwrap();

            simulation.step();
        }

        for _ in 0..7 {
            if manual {
                assert_eq!(
                    command_const(&mut planned, planned_train_id),
                    command_const(&mut injected, injected_train_id),
                );
            }

            assert_eq!(planned.snapshot().trains, injected.snapshot().trains,);

            assert_eq!(claims(&planned), claims(&injected),);

            planned.step();
            injected.step();
        }
    }
}

#[test]
fn future_registration_preserves_present_admission() {
    for manual in [false, true] {
        let (mut plain, plain_train_id) = world(manual, 0, Direction::Forward, 1);

        let (mut constrained, constrained_train_id) = world(manual, 0, Direction::Forward, 1);

        schedule(&mut constrained, closure(0), 1, None);

        if manual {
            assert_eq!(
                command_const(&mut constrained, constrained_train_id),
                command_const(&mut plain, plain_train_id),
            );
        } else {
            constrained.step();
            plain.step();
        }

        assert_eq!(constrained.snapshot().trains, plain.snapshot().trains,);

        assert_eq!(claims(&constrained), claims(&plain),);
    }
}

#[test]
fn removal_and_accelerate_observe_serial_call_order() {
    for remove_first in [false, true] {
        let (mut simulation, train_id) = world(true, 0, Direction::Forward, 1);

        let constraint_id = schedule(&mut simulation, closure(0), 1, None);

        simulation.step();

        if !remove_first {
            assert_exact_command(&mut simulation, train_id, Err(CommandError::Blocked));
        }

        simulation.remove_constraint(constraint_id).unwrap();

        assert!(!moving_const(&simulation, train_id));

        command_const(&mut simulation, train_id).unwrap();
    }
}

#[test]
fn automatic_constraints_are_frozen_for_all_world_n_proposals() {
    for reversed in [false, true] {
        for time in 10..=12 {
            let (mut simulation, first_id) = world(false, 0, Direction::Forward, 1);

            let second_id = simulation
                .add_train(Train::new(100, StationId(2), Direction::Backward, 1))
                .unwrap();

            if reversed {
                simulation.reverse_train_order_for_test();
            }

            simulation.elapsed_seconds = 10;

            schedule(&mut simulation, closure(1), 11, Some(12));

            simulation.elapsed_seconds = time;
            simulation.step();

            assert_eq!(moving_const(&simulation, first_id), time != 11,);

            assert_eq!(moving_const(&simulation, second_id), time != 11,);
        }
    }
}

#[test]
fn constraint_reopening_preserves_conga_progression() {
    for remove in [false, true] {
        for reversed in [false, true] {
            let (mut simulation, first_id) = world(false, 0, Direction::Forward, 2);

            let d = simulation.network.add_station("D");
            simulation.network.connect_bidirectional(StationId(2), d, 3);

            let second_id = simulation
                .add_train(Train::new(100, StationId(1), Direction::Forward, 2))
                .unwrap();

            let third_id = simulation
                .add_train(Train::new(100, StationId(2), Direction::Forward, 2))
                .unwrap();

            let constraint_id = schedule(&mut simulation, closure(2), 1, Some(3));

            if reversed {
                simulation.reverse_train_order_for_test();
            }

            simulation.step();
            simulation.step();

            for train_id in [first_id, second_id, third_id] {
                assert!(!moving_const(&simulation, train_id));
            }

            if remove {
                simulation.remove_constraint(constraint_id).unwrap();
            } else {
                simulation.step();
            }

            for expected in [
                [false, false, true],
                [false, true, true],
                [true, true, true],
            ] {
                simulation.step();

                assert_eq!(
                    [
                        moving_const(&simulation, first_id),
                        moving_const(&simulation, second_id),
                        moving_const(&simulation, third_id),
                    ],
                    expected,
                );
            }
        }
    }
}

#[test]
fn constraint_reopening_preserves_terminal_contention_rules() {
    for remove in [false, true] {
        for reversed in [false, true] {
            for manual in [false, true] {
                let (mut simulation, first_id) = world(manual, 2, Direction::Forward, 2);

                let constructor = if manual {
                    Train::new_manual
                } else {
                    Train::new
                };

                let second_id = simulation
                    .add_train(constructor(100, StationId(2), Direction::Backward, 2))
                    .unwrap();

                let constraint_id = schedule(&mut simulation, closure(2), 1, Some(3));

                if reversed {
                    simulation.reverse_train_order_for_test();
                }

                simulation.step();
                simulation.step();

                if remove {
                    simulation.remove_constraint(constraint_id).unwrap();
                } else {
                    simulation.step();
                }

                if manual {
                    assert_eq!(
                        simulation.apply_command(TrainCommand::Accelerate {
                            train_id: second_id,
                        },),
                        Ok(())
                    );

                    assert_eq!(
                        command_const(&mut simulation, first_id),
                        Err(CommandError::Blocked),
                    );
                } else {
                    simulation.step();
                }

                assert_eq!(moving_const(&simulation, first_id), !manual,);

                assert_eq!(moving_const(&simulation, second_id), manual,);
            }
        }
    }
}

#[test]
fn active_constraints_preserve_manual_validation_precedence() {
    let (mut simulation, train_id) = world(false, 0, Direction::Forward, 3);

    schedule(&mut simulation, closure(0), 1, None);
    simulation.step();

    assert_eq!(
        simulation.apply_command(TrainCommand::Accelerate {
            train_id: TrainId(99),
        }),
        Err(CommandError::UnknownTrain),
    );

    assert_exact_command(&mut simulation, train_id, Err(CommandError::NotManual));

    let mut network = Network::new();
    let a = network.add_station("A");

    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());

    let train_id = simulation
        .add_train(Train::new_manual(100, a, Direction::Forward, 3))
        .unwrap();

    schedule(&mut simulation, closure(0), 1, None);
    simulation.step();

    assert_exact_command(
        &mut simulation,
        train_id,
        Err(CommandError::NoOutgoingTrack),
    );
}
