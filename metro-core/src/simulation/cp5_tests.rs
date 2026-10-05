use super::*;
use crate::domain::train::Direction;

fn world(manual: bool, station: usize, direction: Direction, dwell: u64) -> Simulation {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");
    network.connect_bidirectional(a, b, 3);
    network.connect_bidirectional(b, c, 3);
    let constructor = if manual {
        Train::new_manual
    } else {
        Train::new
    };
    Simulation::new(
        network,
        vec![constructor(100, StationId(station), direction, dwell)],
        DwellPolicy::new(),
    )
}
fn closure(station: usize) -> OperationalConstraint {
    OperationalConstraint::StationUnavailable {
        station: StationId(station),
    }
}
fn schedule(
    s: &mut Simulation,
    value: OperationalConstraint,
    start: u64,
    end: Option<u64>,
) -> ConstraintId {
    s.create_constraint_at(value, start, end, ConstraintOrigin::Planned)
        .unwrap()
}
fn command(s: &mut Simulation) -> Result<(), CommandError> {
    s.apply_command(TrainCommand::Accelerate {
        train_id: TrainId(0),
    })
}
fn moving(s: &Simulation, id: usize) -> bool {
    matches!(
        s.trains
            .iter()
            .find(|e| e.id == TrainId(id))
            .unwrap()
            .train
            .state(),
        TrainState::Moving { .. }
    )
}
fn claims(s: &Simulation) -> ResourceView<TrainId> {
    ResourceView::derive(s.trains.iter().map(|e| (e.id, &e.train)))
}
fn assert_exact_command(s: &mut Simulation, expected: Result<(), CommandError>) {
    let before = format!("{s:?}");
    let physical = claims(s);
    assert_eq!(command(s), expected);
    assert_eq!(format!("{s:?}"), before);
    assert_eq!(claims(s), physical);
}

#[test]
fn scheduled_admission_obeys_exact_activation_and_expiry() {
    for manual in [false, true] {
        for time in 10..=15 {
            let mut s = world(manual, 0, Direction::Forward, 1);
            s.elapsed_seconds = 10;
            schedule(&mut s, closure(0), 11, Some(15));
            // Independent committed-world fixtures keep each candidate physically free.
            s.elapsed_seconds = time;
            assert_eq!(
                s.active_constraint_view()
                    .permits_departure(StationId(0), StationId(1)),
                !(11..15).contains(&time)
            );
            if manual {
                assert_eq!(
                    command(&mut s),
                    if (11..15).contains(&time) {
                        Err(CommandError::Blocked)
                    } else {
                        Ok(())
                    }
                );
            } else {
                s.step();
            }
            assert_eq!(moving(&s, 0), !(11..15).contains(&time));
            if time == 10 {
                if manual {
                    s.step();
                }
                assert_eq!(s.elapsed_seconds, 11);
                assert!(
                    !s.active_constraint_view()
                        .permits_departure(StationId(0), StationId(1))
                );
            }
        }
    }
}

#[test]
fn single_step_constraint_restricts_only_step_beginning_at_start() {
    for time in 10..=12 {
        let mut s = world(false, 0, Direction::Forward, 1);
        s.elapsed_seconds = 10;
        schedule(&mut s, closure(0), 11, Some(12));
        s.elapsed_seconds = time;
        s.step();
        assert_eq!(moving(&s, 0), time != 11);
    }
}

#[test]
fn automatic_ready_train_retries_only_on_later_steps() {
    let mut s = world(false, 0, Direction::Forward, 2);
    schedule(&mut s, closure(0), 1, Some(4));
    s.step();
    for time in 2..=4 {
        s.step();
        assert_eq!(s.elapsed_seconds, time);
        assert_eq!(
            s.trains[0].train.state(),
            TrainState::AtStation {
                station: StationId(0),
                state: AtStationState::Ready
            }
        );
        assert_eq!(s.trains[0].train.velocity(), 0);
    }
    assert!(s.constraints.is_empty());
    s.step();
    assert!(moving(&s, 0));
}

#[test]
fn operational_manual_rejection_preserves_entire_world() {
    // Terminal reversal would change direction if incorrectly committed.
    let mut s = world(true, 2, Direction::Forward, 3);
    schedule(&mut s, closure(2), 1, None);
    s.step();
    assert_exact_command(&mut s, Err(CommandError::Blocked));
}

#[test]
fn clearing_constraint_does_not_replay_manual_intent() {
    for remove in [false, true] {
        let mut s = world(true, 0, Direction::Forward, 1);
        let id = schedule(&mut s, closure(0), 1, Some(3));
        s.step();
        assert_exact_command(&mut s, Err(CommandError::Blocked));
        if remove {
            s.remove_constraint(id).unwrap();
        }
        while s.elapsed_seconds < 5 {
            s.step();
            assert!(!moving(&s, 0));
        }
        assert_eq!(command(&mut s), Ok(()));
    }
}

fn check_action(value: OperationalConstraint, station: usize, direction: Direction, allowed: bool) {
    for manual in [false, true] {
        let mut s = world(manual, station, direction, 2);
        schedule(&mut s, value, 1, None);
        s.step();
        let before_direction = s.trains[0].train.direction();
        if manual {
            assert_eq!(
                command(&mut s),
                if allowed {
                    Ok(())
                } else {
                    Err(CommandError::Blocked)
                }
            );
        } else {
            s.step();
        }
        assert_eq!(moving(&s, 0), allowed);
        if !allowed {
            assert_eq!(s.trains[0].train.direction(), before_direction);
            assert!(
                matches!(s.trains[0].train.state(), TrainState::AtStation { station: at, .. } if at == StationId(station))
            );
        }
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
    let mut plain = world(true, 0, Direction::Forward, 1);
    let mut s = world(true, 0, Direction::Forward, 1);
    for sim in [&mut plain, &mut s] {
        command(sim).unwrap();
        sim.add_train(Train::new_manual(100, StationId(2), Direction::Forward, 1));
    }
    let physical = claims(&s);
    for value in [
        OperationalConstraint::TrackUnavailable {
            from: StationId(0),
            to: StationId(1),
        },
        closure(1),
        closure(2),
    ] {
        schedule(&mut s, value, 1, None);
    }
    assert_eq!(claims(&s), physical);
    for _ in 0..5 {
        plain.step();
        s.step();
        assert_eq!(s.snapshot().trains, plain.snapshot().trains);
        assert_eq!(claims(&s), claims(&plain));
    }
    assert!(matches!(
        s.trains[0].train.state(),
        TrainState::AtStation {
            station: StationId(1),
            ..
        }
    ));
    assert_exact_command(&mut s, Err(CommandError::Blocked));
    crate::test_utils::utils::assert_physical_invariants(&s);
}

#[test]
fn occupied_station_keeps_dwell_and_blocks_new_departure_until_reopening() {
    let mut s = world(false, 1, Direction::Forward, 3);
    schedule(&mut s, closure(1), 1, Some(4));

    for expected_time in 1..=4 {
        s.step();
        crate::test_utils::utils::assert_physical_invariants(&s);
        assert_eq!(s.elapsed_seconds, expected_time);
        assert!(matches!(
            s.trains[0].train.state(),
            TrainState::AtStation {
                station: StationId(1),
                ..
            }
        ));
    }
    assert!(s.snapshot().constraints.is_empty());
    s.step();
    crate::test_utils::utils::assert_physical_invariants(&s);
    assert!(matches!(
        s.trains[0].train.state(),
        TrainState::Moving {
            from: StationId(1),
            to: StationId(2),
            elapsed_seconds: 0
        }
    ));
}

#[test]
fn moving_manual_accelerate_is_exact_noop_under_active_constraints() {
    let mut s = world(true, 0, Direction::Forward, 1);
    schedule(&mut s, closure(1), 1, None);
    command(&mut s).unwrap();
    s.step();
    assert_exact_command(&mut s, Ok(()));
}

#[test]
fn overlapping_causes_block_until_last_cause_clears() {
    for duplicate in [false, true] {
        for remove in [false, true] {
            let mut s = world(true, 0, Direction::Forward, 1);
            let first = schedule(&mut s, closure(0), 1, Some(2));
            let second_value = if duplicate {
                closure(0)
            } else {
                OperationalConstraint::TrackUnavailable {
                    from: StationId(0),
                    to: StationId(1),
                }
            };
            let second = schedule(&mut s, second_value, 1, None);
            s.step();
            if remove {
                s.remove_constraint(first).unwrap();
            } else {
                s.step();
            }
            assert_exact_command(&mut s, Err(CommandError::Blocked));
            s.remove_constraint(second).unwrap();
            command(&mut s).unwrap();
        }
    }
}

#[test]
fn equal_time_handoff_keeps_duplicate_causes_continuously_effective() {
    let mut s = world(true, 0, Direction::Forward, 1);
    let ending_at_five = s
        .create_constraint_at(closure(0), 1, Some(5), ConstraintOrigin::Planned)
        .unwrap();
    let starting_at_five = s
        .create_constraint_at(closure(0), 5, Some(8), ConstraintOrigin::Injected)
        .unwrap();
    let removed_duplicate = s
        .create_constraint_at(closure(0), 1, Some(4), ConstraintOrigin::Injected)
        .unwrap();
    let expiring_duplicate = s
        .create_constraint_at(closure(0), 1, Some(6), ConstraintOrigin::Planned)
        .unwrap();

    for _ in 0..3 {
        s.step();
        crate::test_utils::utils::assert_physical_invariants(&s);
    }
    assert_eq!(s.elapsed_seconds, 3);
    assert_eq!(
        s.snapshot()
            .constraints
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![
            ending_at_five,
            starting_at_five,
            removed_duplicate,
            expiring_duplicate
        ]
    );

    s.remove_constraint(removed_duplicate).unwrap();
    assert_exact_command(&mut s, Err(CommandError::Blocked));
    s.step();
    crate::test_utils::utils::assert_physical_invariants(&s);
    assert_exact_command(&mut s, Err(CommandError::Blocked));

    s.step();
    crate::test_utils::utils::assert_physical_invariants(&s);
    assert_eq!(s.elapsed_seconds, 5);
    assert_eq!(
        s.snapshot()
            .constraints
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![starting_at_five, expiring_duplicate]
    );
    assert_exact_command(&mut s, Err(CommandError::Blocked));

    s.step();
    crate::test_utils::utils::assert_physical_invariants(&s);
    assert_eq!(s.elapsed_seconds, 6);
    assert_eq!(s.snapshot().constraints[0].id, starting_at_five);
    assert_exact_command(&mut s, Err(CommandError::Blocked));
    s.step();
    crate::test_utils::utils::assert_physical_invariants(&s);
    assert_exact_command(&mut s, Err(CommandError::Blocked));

    s.step();
    crate::test_utils::utils::assert_physical_invariants(&s);
    assert_eq!(s.elapsed_seconds, 8);
    assert!(s.snapshot().constraints.is_empty());
    command(&mut s).unwrap();
    crate::test_utils::utils::assert_physical_invariants(&s);

    let next = s
        .create_constraint_at(closure(0), 9, None, ConstraintOrigin::Injected)
        .unwrap();
    assert_eq!(next.0, 4);
}

#[test]
fn terminal_reversal_retries_after_operational_expiry() {
    let mut s = world(false, 2, Direction::Forward, 3);
    schedule(
        &mut s,
        OperationalConstraint::TrackUnavailable {
            from: StationId(2),
            to: StationId(1),
        },
        1,
        Some(4),
    );

    for expected_time in 1..=4 {
        s.step();
        crate::test_utils::utils::assert_physical_invariants(&s);
        assert_eq!(s.elapsed_seconds, expected_time);
        if expected_time < 4 {
            assert_eq!(s.trains[0].train.direction(), Direction::Forward);
            assert!(matches!(
                s.trains[0].train.state(),
                TrainState::AtStation {
                    station: StationId(2),
                    ..
                }
            ));
            if expected_time >= 3 {
                assert!(matches!(
                    s.trains[0].train.state(),
                    TrainState::AtStation {
                        state: AtStationState::Ready,
                        ..
                    }
                ));
            }
        }
    }

    assert!(s.snapshot().constraints.is_empty());
    assert_eq!(s.trains[0].train.direction(), Direction::Forward);
    assert!(matches!(
        s.trains[0].train.state(),
        TrainState::AtStation {
            station: StationId(2),
            state: AtStationState::Ready
        }
    ));
    s.step();
    crate::test_utils::utils::assert_physical_invariants(&s);
    assert_eq!(s.trains[0].train.direction(), Direction::Backward);
    assert!(matches!(
        s.trains[0].train.state(),
        TrainState::Moving {
            from: StationId(2),
            to: StationId(1),
            elapsed_seconds: 0
        }
    ));
}

#[test]
fn constraint_origin_does_not_change_admission_or_claim_traces() {
    for manual in [false, true] {
        let mut planned = world(manual, 0, Direction::Forward, 2);
        let mut injected = world(manual, 0, Direction::Forward, 2);
        for (s, origin) in [
            (&mut planned, ConstraintOrigin::Planned),
            (&mut injected, ConstraintOrigin::Injected),
        ] {
            s.create_constraint_at(closure(0), 1, Some(4), origin)
                .unwrap();
            s.step();
        }
        for _ in 0..7 {
            if manual {
                assert_eq!(command(&mut planned), command(&mut injected));
            }
            assert_eq!(planned.snapshot().trains, injected.snapshot().trains);
            assert_eq!(claims(&planned), claims(&injected));
            planned.step();
            injected.step();
        }
    }
}

#[test]
fn future_registration_preserves_present_admission() {
    for manual in [false, true] {
        let mut plain = world(manual, 0, Direction::Forward, 1);
        let mut s = world(manual, 0, Direction::Forward, 1);
        schedule(&mut s, closure(0), 1, None);
        if manual {
            assert_eq!(command(&mut s), command(&mut plain));
        } else {
            s.step();
            plain.step();
        }
        assert_eq!(s.snapshot().trains, plain.snapshot().trains);
        assert_eq!(claims(&s), claims(&plain));
    }
}

#[test]
fn removal_and_accelerate_observe_serial_call_order() {
    for remove_first in [false, true] {
        let mut s = world(true, 0, Direction::Forward, 1);
        let id = schedule(&mut s, closure(0), 1, None);
        s.step();
        if !remove_first {
            assert_exact_command(&mut s, Err(CommandError::Blocked));
        }
        s.remove_constraint(id).unwrap();
        assert!(!moving(&s, 0));
        command(&mut s).unwrap();
    }
}

#[test]
fn automatic_constraints_are_frozen_for_all_world_n_proposals() {
    for reversed in [false, true] {
        for time in 10..=12 {
            let mut s = world(false, 0, Direction::Forward, 1);
            s.add_train(Train::new(100, StationId(2), Direction::Backward, 1));
            if reversed {
                s.reverse_train_order_for_test();
            }
            s.elapsed_seconds = 10;
            schedule(&mut s, closure(1), 11, Some(12));
            s.elapsed_seconds = time;
            s.step();
            for id in 0..2 {
                assert_eq!(moving(&s, id), time != 11);
            }
        }
    }
}

#[test]
fn constraint_reopening_preserves_conga_and_contention() {
    for remove in [false, true] {
        for reversed in [false, true] {
            // Reopening a queue must not release all source claims in one step.
            let mut s = world(false, 0, Direction::Forward, 2);
            let d = s.network.add_station("D");
            s.network.connect_bidirectional(StationId(2), d, 3);
            s.add_train(Train::new(100, StationId(1), Direction::Forward, 2));
            s.add_train(Train::new(100, StationId(2), Direction::Forward, 2));
            let id = schedule(&mut s, closure(2), 1, Some(3));
            if reversed {
                s.reverse_train_order_for_test();
            }
            s.step();
            s.step();
            for id in 0..3 {
                assert!(!moving(&s, id));
            }
            if remove {
                s.remove_constraint(id).unwrap();
            } else {
                s.step();
            }
            for expected in [
                vec![false, false, true],
                vec![false, true, true],
                vec![true, true, true],
            ] {
                s.step();
                assert_eq!(
                    (0..3).map(|id| moving(&s, id)).collect::<Vec<_>>(),
                    expected
                );
            }

            // Same-track and destination-slot contention at a terminal.
            for manual in [false, true] {
                let mut s = world(manual, 2, Direction::Forward, 2);
                let constructor = if manual {
                    Train::new_manual
                } else {
                    Train::new
                };
                s.add_train(constructor(100, StationId(2), Direction::Backward, 2));
                let id = schedule(&mut s, closure(2), 1, Some(3));
                if reversed {
                    s.reverse_train_order_for_test();
                }
                s.step();
                s.step();
                if remove {
                    s.remove_constraint(id).unwrap();
                } else {
                    s.step();
                }
                if manual {
                    // Higher numeric ID wins when its serial command arrives first.
                    assert_eq!(
                        s.apply_command(TrainCommand::Accelerate {
                            train_id: TrainId(1)
                        }),
                        Ok(())
                    );
                    assert_eq!(command(&mut s), Err(CommandError::Blocked));
                } else {
                    s.step();
                }
                assert_eq!(moving(&s, 0), !manual);
                assert_eq!(moving(&s, 1), manual);
            }
        }
    }
}

#[test]
fn active_constraints_preserve_manual_validation_precedence() {
    let mut s = world(false, 0, Direction::Forward, 3);
    schedule(&mut s, closure(0), 1, None);
    s.step();
    assert_eq!(
        s.apply_command(TrainCommand::Accelerate {
            train_id: TrainId(99)
        }),
        Err(CommandError::UnknownTrain)
    );
    assert_exact_command(&mut s, Err(CommandError::NotManual));
    let mut network = Network::new();
    let a = network.add_station("A");
    let mut s = Simulation::new(
        network,
        vec![Train::new_manual(100, a, Direction::Forward, 3)],
        DwellPolicy::new(),
    );
    schedule(&mut s, closure(0), 1, None);
    s.step();
    assert_exact_command(&mut s, Err(CommandError::NoOutgoingTrack));
}
