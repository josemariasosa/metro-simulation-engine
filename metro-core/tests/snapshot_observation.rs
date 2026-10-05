use metro_core::DwellPolicy;
use metro_core::Network;
use metro_core::StationId;
use metro_core::simulation::Simulation;
use metro_core::snapshot::{SimulationSnapshot, TrainSnapshot, TrainSnapshotState};
use metro_core::{
    ConstraintError, ConstraintOrigin, ConstraintSnapshot, Direction, OperationalConstraint, Train,
    TrainId,
};

fn test_simulation(travel_seconds: u64) -> Simulation {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, travel_seconds);
    let train = Train::new(100, a, Direction::Forward, 3);
    Simulation::new(network, vec![train], DwellPolicy::new())
}

fn no_train_simulation() -> (Simulation, StationId, StationId) {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 6);
    (Simulation::new(network, vec![], DwellPolicy::new()), a, b)
}

fn track_constraint(from: StationId, to: StationId) -> OperationalConstraint {
    OperationalConstraint::TrackUnavailable { from, to }
}

fn expected_snapshot(
    elapsed_seconds: u64,
    velocity: u8,
    state: TrainSnapshotState,
) -> SimulationSnapshot {
    SimulationSnapshot {
        elapsed_seconds,
        trains: vec![TrainSnapshot {
            id: TrainId(0),
            direction: Direction::Forward,
            state,
            velocity,
        }],
        constraints: vec![],
    }
}

#[test]
fn repeated_snapshots_without_step_are_equal() {
    let simulation = test_simulation(6);

    let first = simulation.snapshot();
    let second = simulation.snapshot();

    assert_eq!(first, second);
}

#[test]
fn retained_snapshot_does_not_change_after_simulation_advances() {
    let mut simulation = test_simulation(6);

    let old_snapshot = simulation.snapshot();
    let expected = old_snapshot.clone();

    simulation.step();
    simulation.step();
    simulation.step();

    assert_eq!(old_snapshot, expected);
    assert_ne!(simulation.snapshot(), old_snapshot);
}

#[test]
fn modifying_snapshot_does_not_modify_simulation() {
    let simulation = test_simulation(6);

    let original = simulation.snapshot();
    let mut local_copy = simulation.snapshot();

    local_copy.elapsed_seconds = 999;
    local_copy.trains.clear();

    assert_ne!(local_copy, original);
    assert_eq!(simulation.snapshot(), original);
}

#[test]
fn observation_frequency_does_not_affect_simulation_result() {
    let mut frequently_observed = test_simulation(6);
    let mut sparsely_observed = test_simulation(6);

    for _ in 0..12 {
        frequently_observed.snapshot();
        frequently_observed.snapshot();

        frequently_observed.step();
        sparsely_observed.step();
    }

    assert_eq!(frequently_observed.snapshot(), sparsely_observed.snapshot());
}

#[test]
fn snapshot_reports_empty_simulation() {
    let mut simulation = Simulation::new(Network::new(), vec![], DwellPolicy::new());
    simulation.step();

    assert_eq!(
        simulation.snapshot(),
        SimulationSnapshot {
            elapsed_seconds: 1,
            trains: vec![],
            constraints: vec![],
        }
    );
}

#[test]
fn snapshot_reports_initial_dwelling_state() {
    let simulation = test_simulation(6);

    assert_eq!(
        simulation.snapshot(),
        expected_snapshot(
            0,
            0,
            TrainSnapshotState::Dwelling {
                station: StationId(0),
                remaining_seconds: 3,
            }
        )
    );
}

#[test]
fn snapshot_represents_ready_manual_train() {
    let mut network = Network::new();
    let station_a = network.add_station("A");
    let station_b = network.add_station("B");
    network.connect_bidirectional(station_a, station_b, 6);
    let train = Train::new_manual(100, station_a, Direction::Forward, 3);
    let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());

    for _ in 0..3 {
        simulation.step();
    }

    let snapshot = simulation.snapshot();

    assert_eq!(
        snapshot.trains[0].state,
        TrainSnapshotState::Ready { station: station_a }
    );
    assert_eq!(snapshot.trains[0].velocity, 0);
}

#[test]
fn snapshot_reports_departure_with_zero_elapsed_time() {
    let mut simulation = test_simulation(6);
    for _ in 0..3 {
        simulation.step();
    }

    assert_eq!(
        simulation.snapshot(),
        expected_snapshot(
            3,
            1,
            TrainSnapshotState::Moving {
                from: StationId(0),
                to: StationId(1),
                elapsed_seconds: 0,
                travel_seconds: 6,
            }
        )
    );
}

#[test]
fn snapshot_reports_intermediate_movement() {
    let mut simulation = test_simulation(6);
    for _ in 0..5 {
        simulation.step();
    }

    assert_eq!(
        simulation.snapshot(),
        expected_snapshot(
            5,
            1,
            TrainSnapshotState::Moving {
                from: StationId(0),
                to: StationId(1),
                elapsed_seconds: 2,
                travel_seconds: 6,
            }
        )
    );
}

#[test]
fn snapshot_reports_arrival_as_dwelling() {
    let mut simulation = test_simulation(6);
    for _ in 0..9 {
        simulation.step();
    }

    assert_eq!(
        simulation.snapshot(),
        expected_snapshot(
            9,
            0,
            TrainSnapshotState::Dwelling {
                station: StationId(1),
                remaining_seconds: 3,
            }
        )
    );
}

#[test]
fn snapshot_reports_departure_and_arrival_on_one_second_track() {
    let mut simulation = test_simulation(1);
    for _ in 0..3 {
        simulation.step();
    }

    assert_eq!(
        simulation.snapshot(),
        expected_snapshot(
            3,
            1,
            TrainSnapshotState::Moving {
                from: StationId(0),
                to: StationId(1),
                elapsed_seconds: 0,
                travel_seconds: 1,
            }
        )
    );

    simulation.step();

    assert_eq!(
        simulation.snapshot(),
        expected_snapshot(
            4,
            0,
            TrainSnapshotState::Dwelling {
                station: StationId(1),
                remaining_seconds: 3,
            }
        )
    );
}

#[test]
fn snapshot_preserves_train_order_and_uses_each_active_track() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.add_track(a, b, 6);
    network.add_track(b, a, 4);
    let trains = vec![
        Train::new(100, b, Direction::Backward, 3),
        Train::new(100, a, Direction::Forward, 3),
    ];
    let mut simulation = Simulation::new(network, trains, DwellPolicy::new());
    for _ in 0..3 {
        simulation.step();
    }

    assert_eq!(
        simulation.snapshot(),
        SimulationSnapshot {
            elapsed_seconds: 3,
            trains: vec![
                TrainSnapshot {
                    id: TrainId(0),
                    direction: Direction::Backward,
                    velocity: 1,
                    state: TrainSnapshotState::Moving {
                        from: b,
                        to: a,
                        elapsed_seconds: 0,
                        travel_seconds: 4,
                    },
                },
                TrainSnapshot {
                    id: TrainId(1),
                    direction: Direction::Forward,
                    velocity: 1,
                    state: TrainSnapshotState::Moving {
                        from: a,
                        to: b,
                        elapsed_seconds: 0,
                        travel_seconds: 6,
                    },
                },
            ],
            constraints: vec![],
        }
    );
}

#[test]
fn empty_snapshot_has_no_constraints_and_no_train_simulation_can_observe_them() {
    let (mut simulation, a, b) = no_train_simulation();
    assert!(simulation.snapshot().constraints.is_empty());

    let id = simulation
        .create_constraint_at(track_constraint(a, b), 2, None, ConstraintOrigin::Planned)
        .unwrap();
    assert_eq!(simulation.snapshot().constraints[0].id, id);
    simulation.step();
    assert!(simulation.snapshot().elapsed_seconds < simulation.snapshot().constraints[0].start_at);
    simulation.step();
    assert_eq!(simulation.snapshot().constraints[0].start_at, 2);
    assert_eq!(simulation.snapshot().elapsed_seconds, 2);
    assert!(simulation.snapshot().trains.is_empty());
}

#[test]
fn scheduled_and_active_records_are_visible_with_exact_fields_and_origin() {
    let (mut simulation, a, b) = no_train_simulation();
    let active_id = simulation
        .create_constraint_at(
            track_constraint(a, b),
            1,
            Some(4),
            ConstraintOrigin::Planned,
        )
        .unwrap();
    let scheduled_id = simulation
        .create_constraint_at(track_constraint(a, b), 3, None, ConstraintOrigin::Injected)
        .unwrap();

    let before_start = simulation.snapshot();
    assert_eq!(before_start.constraints.len(), 2);
    assert_eq!(before_start, simulation.snapshot());
    assert!(
        before_start
            .constraints
            .iter()
            .all(|record| { before_start.elapsed_seconds < record.start_at })
    );

    simulation.step();
    let at_start = simulation.snapshot();
    assert_eq!(at_start.elapsed_seconds, 1);
    assert_eq!(
        at_start.constraints,
        vec![
            ConstraintSnapshot {
                id: active_id,
                constraint: track_constraint(a, b),
                start_at: 1,
                end_at: Some(4),
                origin: ConstraintOrigin::Planned,
            },
            ConstraintSnapshot {
                id: scheduled_id,
                constraint: track_constraint(a, b),
                start_at: 3,
                end_at: None,
                origin: ConstraintOrigin::Injected,
            },
        ]
    );
    assert!(at_start.elapsed_seconds >= at_start.constraints[0].start_at);
    assert!(at_start.elapsed_seconds < at_start.constraints[0].end_at.unwrap());
    assert!(at_start.elapsed_seconds < at_start.constraints[1].start_at);
}

#[test]
fn finite_constraint_is_absent_at_end_boundary_after_normal_step() {
    let (mut simulation, a, b) = no_train_simulation();
    let id = simulation
        .create_constraint_at(
            track_constraint(a, b),
            1,
            Some(3),
            ConstraintOrigin::Injected,
        )
        .unwrap();
    for _ in 0..3 {
        simulation.step();
    }

    assert_eq!(simulation.elapsed_seconds, 3);
    assert!(simulation.snapshot().constraints.is_empty());
    assert_eq!(
        simulation.remove_constraint(id),
        Err(ConstraintError::UnknownConstraint)
    );
}

#[test]
fn removing_scheduled_or_active_constraint_keeps_unrelated_records_observable() {
    let (mut simulation, a, b) = no_train_simulation();
    let scheduled = simulation
        .create_constraint_at(track_constraint(a, b), 5, None, ConstraintOrigin::Planned)
        .unwrap();
    let active = simulation
        .create_constraint_at(track_constraint(b, a), 1, None, ConstraintOrigin::Injected)
        .unwrap();
    let unrelated = simulation
        .create_constraint_at(
            OperationalConstraint::StationDeparturesBlocked { station: a },
            7,
            None,
            ConstraintOrigin::Planned,
        )
        .unwrap();
    simulation.step();

    simulation.remove_constraint(scheduled).unwrap();
    assert_eq!(
        simulation
            .snapshot()
            .constraints
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![active, unrelated]
    );
    simulation.remove_constraint(active).unwrap();
    assert_eq!(
        simulation
            .snapshot()
            .constraints
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![unrelated]
    );
}

#[test]
fn duplicate_values_remain_distinct_and_indefinite_records_persist_until_removed() {
    let (mut simulation, a, b) = no_train_simulation();
    let value = track_constraint(a, b);
    let first = simulation
        .create_constraint_at(value, 1, Some(3), ConstraintOrigin::Planned)
        .unwrap();
    let second = simulation
        .create_constraint_at(value, 2, None, ConstraintOrigin::Injected)
        .unwrap();

    simulation.step();
    simulation.step();
    let both_registered = simulation.snapshot();
    assert_eq!(both_registered.constraints.len(), 2);
    assert_eq!(both_registered.constraints[0].id, first);
    assert_eq!(both_registered.constraints[1].id, second);
    assert_eq!(both_registered.constraints[0].constraint, value);
    assert_eq!(both_registered.constraints[1].constraint, value);
    assert_eq!(both_registered.constraints[0].end_at, Some(3));
    assert_eq!(both_registered.constraints[1].end_at, None);

    simulation.remove_constraint(first).unwrap();
    assert_eq!(simulation.snapshot().constraints.len(), 1);
    assert_eq!(simulation.snapshot().constraints[0].id, second);

    for _ in 0..3 {
        simulation.step();
    }
    let snapshot = simulation.snapshot();
    assert_eq!(snapshot.constraints.len(), 1);
    assert_eq!(snapshot.constraints[0].id, second);
    assert_eq!(snapshot.constraints[0].constraint, value);
    assert_eq!(snapshot.constraints[0].end_at, None);
    assert_eq!(snapshot.constraints[0].origin, ConstraintOrigin::Injected);

    simulation.remove_constraint(second).unwrap();
    assert!(simulation.snapshot().constraints.is_empty());
}

#[test]
fn retained_and_mutated_snapshot_values_do_not_change_simulation() {
    let mut simulation = test_simulation(6);
    let (a, b) = (StationId(0), StationId(1));
    let retained_id = simulation
        .create_constraint_at(track_constraint(a, b), 8, None, ConstraintOrigin::Planned)
        .unwrap();
    let mut retained = simulation.snapshot();
    let original = retained.clone();

    simulation.step();
    simulation.remove_constraint(retained_id).unwrap();
    simulation
        .create_constraint_at(track_constraint(b, a), 9, None, ConstraintOrigin::Injected)
        .unwrap();
    let later = simulation.snapshot();
    assert_eq!(retained, original);
    assert_ne!(retained, later);

    retained.elapsed_seconds = u64::MAX;
    retained.constraints.clear();
    retained.trains.clear();
    assert_eq!(simulation.snapshot(), later);
}

#[test]
fn snapshot_copies_train_velocity() {
    let mut simulation = test_simulation(6);

    let initial_snapshot = simulation.snapshot();
    assert_eq!(initial_snapshot.trains[0].velocity, 0);

    for _ in 0..3 {
        simulation.step();
    }

    let departure_snapshot = simulation.snapshot();
    assert_eq!(departure_snapshot.trains[0].velocity, 1);
    assert_eq!(initial_snapshot.trains[0].velocity, 0);
}
