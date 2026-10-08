use std::collections::HashSet;

use metro_core::{
    ConstraintError, ConstraintId, ConstraintOrigin, Direction, DwellPolicy, Network,
    OperationalConstraint, Train,
    command::{CommandError, TrainCommand},
    simulation::Simulation,
    snapshot::{SimulationSnapshot, TrainSnapshotState},
};

fn assert_snapshot_claims(snapshot: &SimulationSnapshot, stations: usize, travel_seconds: u64) {
    let mut occupied_slots = HashSet::new();
    let mut directed_tracks = HashSet::new();
    for train in &snapshot.trains {
        match train.state {
            TrainSnapshotState::Dwelling { station, .. }
            | TrainSnapshotState::Ready { station } => {
                assert!(station.0 < stations);
                assert!(occupied_slots.insert((station, train.direction)));
            }
            TrainSnapshotState::Moving {
                from,
                to,
                elapsed_seconds,
                travel_seconds: actual_travel_seconds,
            } => {
                assert!(from.0 < stations && to.0 < stations);
                assert_eq!(actual_travel_seconds, travel_seconds);
                assert!(elapsed_seconds < actual_travel_seconds);
                assert!(directed_tracks.insert((from, to)));
                assert!(occupied_slots.insert((to, train.direction)));
            }
        }
    }
}

fn run_reopening_conga(remove_at_five: bool) -> Vec<SimulationSnapshot> {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");
    let d = network.add_station("D");
    network.connect_bidirectional(a, b, 3);
    network.connect_bidirectional(b, c, 3);
    network.connect_bidirectional(c, d, 3);

    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let ids = [
        simulation
            .add_train(Train::new(100, a, Direction::Forward, 3))
            .unwrap(),
        simulation
            .add_train(Train::new(100, b, Direction::Forward, 3))
            .unwrap(),
        simulation
            .add_train(Train::new(100, c, Direction::Forward, 3))
            .unwrap(),
    ];
    let constraint = simulation
        .create_constraint_at(
            OperationalConstraint::TrackUnavailable { from: c, to: d },
            1,
            if remove_at_five { None } else { Some(5) },
            if remove_at_five {
                ConstraintOrigin::Injected
            } else {
                ConstraintOrigin::Planned
            },
        )
        .unwrap();

    let mut trace = vec![simulation.snapshot()];
    while simulation.elapsed_seconds < 5 {
        simulation.step();
        let snapshot = simulation.snapshot();
        assert_snapshot_claims(&snapshot, 4, 3);
        trace.push(snapshot);
    }
    assert_eq!(simulation.elapsed_seconds, 5);
    for snapshot in &trace[1..=5] {
        assert!(
            snapshot
                .trains
                .iter()
                .all(|train| !matches!(train.state, TrainSnapshotState::Moving { .. }))
        );
        if (3..=5).contains(&snapshot.elapsed_seconds) {
            assert!(
                snapshot
                    .trains
                    .iter()
                    .all(|train| matches!(train.state, TrainSnapshotState::Ready { .. }))
            );
        }
        assert_eq!(
            snapshot.constraints.is_empty(),
            snapshot.elapsed_seconds == 5 && !remove_at_five
        );
    }
    assert!(simulation.snapshot().constraints.is_empty() == !remove_at_five);
    if remove_at_five {
        simulation.remove_constraint(constraint).unwrap();
        assert!(simulation.snapshot().constraints.is_empty());
    }
    assert!(
        trace
            .last()
            .unwrap()
            .trains
            .iter()
            .all(|train| matches!(train.state, TrainSnapshotState::Ready { .. }))
    );

    for (expected_time, newly_departed) in [(6, ids[2]), (7, ids[1]), (8, ids[0])] {
        simulation.step();
        let snapshot = simulation.snapshot();
        assert_eq!(snapshot.elapsed_seconds, expected_time);
        assert_snapshot_claims(&snapshot, 4, 3);
        for train in &snapshot.trains {
            if train.id == newly_departed {
                assert!(matches!(
                    train.state,
                    TrainSnapshotState::Moving {
                        elapsed_seconds: 0,
                        ..
                    }
                ));
            } else if ids.iter().position(|id| *id == train.id).unwrap()
                < ids.iter().position(|id| *id == newly_departed).unwrap()
            {
                assert!(matches!(train.state, TrainSnapshotState::Ready { .. }));
            }
        }
        trace.push(snapshot);
    }
    trace
}

#[test]
fn public_reopening_conga_is_frozen_across_expiry_and_removal() {
    let expired = run_reopening_conga(false);
    let removed = run_reopening_conga(true);

    assert_eq!(expired.len(), 9);
    assert_eq!(removed.len(), 9);
    assert_eq!(expired[0].constraints[0].start_at, 1);
    assert_eq!(expired[0].constraints[0].end_at, Some(5));
    assert_eq!(expired[5].elapsed_seconds, 5);
    assert!(expired[5].constraints.is_empty());
    assert_eq!(
        expired[5].trains, removed[5].trains,
        "expiry cleanup and explicit removal must not cause a same-step departure"
    );
    for time in 6..=8 {
        assert_eq!(expired[time].trains, removed[time].trains);
    }
}

fn conga_train_trace(injected_at_time_one: bool) -> Vec<Vec<metro_core::snapshot::TrainSnapshot>> {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");
    let d = network.add_station("D");
    network.connect_bidirectional(a, b, 3);
    network.connect_bidirectional(b, c, 3);
    network.connect_bidirectional(c, d, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    for station in [a, b, c] {
        simulation
            .add_train(Train::new(100, station, Direction::Forward, 3))
            .unwrap();
    }
    if !injected_at_time_one {
        simulation
            .create_constraint_at(
                OperationalConstraint::TrackUnavailable { from: c, to: d },
                1,
                Some(5),
                ConstraintOrigin::Planned,
            )
            .unwrap();
    }

    let mut trace = vec![simulation.snapshot().trains];
    simulation.step();
    trace.push(simulation.snapshot().trains);
    if injected_at_time_one {
        simulation
            .create_constraint_in(
                OperationalConstraint::TrackUnavailable { from: c, to: d },
                1,
                Some(3),
                ConstraintOrigin::Injected,
            )
            .unwrap();
    }
    while simulation.elapsed_seconds < 8 {
        simulation.step();
        let snapshot = simulation.snapshot();
        assert_snapshot_claims(&snapshot, 4, 3);
        trace.push(snapshot.trains);
    }
    trace
}

#[test]
fn injected_relative_queue_matches_planned_queue_train_trace() {
    assert_eq!(
        conga_train_trace(false),
        conga_train_trace(true),
        "the injected [2,5) queue cannot affect decisions before its start"
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PublicTrace {
    ids: Vec<ConstraintId>,
    command_results: Vec<Result<(), CommandError>>,
    times: Vec<u64>,
    snapshots: Vec<SimulationSnapshot>,
}

fn run_public_trace(observe_frequently: bool, terminal_origin: ConstraintOrigin) -> PublicTrace {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");
    let d = network.add_station("D");
    network.connect_bidirectional(a, b, 3);
    network.connect_bidirectional(b, c, 3);
    network.connect_bidirectional(c, d, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let forward_terminal = simulation
        .add_train(Train::new(100, d, Direction::Forward, 3))
        .unwrap();
    let backward_terminal = simulation
        .add_train(Train::new(100, d, Direction::Backward, 3))
        .unwrap();
    let manual_train = simulation
        .add_train(Train::new_manual(100, a, Direction::Forward, 10))
        .unwrap();

    let mut ids = Vec::new();
    let mut command_results = Vec::new();
    let mut times = Vec::new();
    let mut snapshots = Vec::new();
    let terminal_closure = simulation
        .create_constraint_at(
            OperationalConstraint::StationUnavailable { station: d },
            1,
            Some(5),
            terminal_origin,
        )
        .unwrap();
    ids.push(terminal_closure);
    let traversal_closure = simulation
        .create_constraint_in(
            OperationalConstraint::TrackUnavailable { from: a, to: b },
            1,
            Some(2),
            ConstraintOrigin::Injected,
        )
        .unwrap();
    ids.push(traversal_closure);
    let canceled = simulation
        .create_constraint_at(
            OperationalConstraint::StationUnavailable { station: c },
            6,
            None,
            ConstraintOrigin::Planned,
        )
        .unwrap();
    ids.push(canceled);
    if observe_frequently {
        let _ = simulation.snapshot();
    }
    snapshots.push(simulation.snapshot());

    command_results.push(simulation.apply_command(TrainCommand::Accelerate {
        train_id: manual_train,
    }));
    if observe_frequently {
        let _ = simulation.snapshot();
    }
    for expected_time in 1..=2 {
        simulation.step();
        times.push(simulation.elapsed_seconds);
        assert_eq!(simulation.elapsed_seconds, expected_time);
        if observe_frequently || expected_time == 1 {
            let _ = simulation.snapshot();
        }
    }
    command_results.push(simulation.apply_command(TrainCommand::Accelerate {
        train_id: manual_train,
    }));
    simulation.remove_constraint(canceled).unwrap();
    snapshots.push(simulation.snapshot());

    for expected_time in 3..=5 {
        simulation.step();
        times.push(simulation.elapsed_seconds);
        assert_eq!(simulation.elapsed_seconds, expected_time);
        if observe_frequently {
            let _ = simulation.snapshot();
        }
        if expected_time == 3 || expected_time == 5 {
            snapshots.push(simulation.snapshot());
        }
    }
    let relative = simulation
        .create_constraint_in(
            OperationalConstraint::StationDeparturesBlocked { station: a },
            1,
            Some(2),
            ConstraintOrigin::Planned,
        )
        .unwrap();
    ids.push(relative);
    assert_eq!(
        simulation.snapshot().constraints.last().unwrap().start_at,
        6
    );

    for expected_time in 6..=10 {
        simulation.step();
        times.push(simulation.elapsed_seconds);
        assert_eq!(simulation.elapsed_seconds, expected_time);
        if observe_frequently {
            let _ = simulation.snapshot();
        }
        if expected_time == 6 || expected_time == 10 {
            snapshots.push(simulation.snapshot());
        }
    }
    let final_snapshot = simulation.snapshot();
    assert!(final_snapshot.constraints.is_empty());
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.elapsed_seconds == 6
            && matches!(
                Simulation::snapshot_train_by_id(snapshot, forward_terminal).state,
                TrainSnapshotState::Moving {
                    from,
                    to,
                    elapsed_seconds: 0,
                    ..
                } if from == d && to == c
            )
    }));
    assert!(matches!(
        Simulation::snapshot_train_by_id(&final_snapshot, backward_terminal).state,
        TrainSnapshotState::Ready { station } if station == d
    ));
    assert_eq!(
        Simulation::snapshot_train_by_id(&final_snapshot, forward_terminal).direction,
        Direction::Backward
    );
    assert!(matches!(
        Simulation::snapshot_train_by_id(&final_snapshot, manual_train).state,
        TrainSnapshotState::Dwelling { station, .. } | TrainSnapshotState::Ready { station }
            if station == b
    ));
    snapshots.push(final_snapshot);

    PublicTrace {
        ids,
        command_results,
        times,
        snapshots,
    }
}

fn normalize_constraint_origins(trace: &mut PublicTrace) {
    for snapshot in &mut trace.snapshots {
        for constraint in &mut snapshot.constraints {
            constraint.origin = ConstraintOrigin::Planned;
        }
    }
}

#[test]
fn replay_and_observation_frequency_do_not_change_constraint_traces() {
    let replay_a = run_public_trace(false, ConstraintOrigin::Planned);
    let replay_b = run_public_trace(false, ConstraintOrigin::Planned);
    assert_eq!(replay_a, replay_b);

    let frequent = run_public_trace(true, ConstraintOrigin::Planned);
    assert_eq!(frequent, replay_a);

    let mut injected = run_public_trace(true, ConstraintOrigin::Injected);
    let mut planned = replay_a;
    normalize_constraint_origins(&mut injected);
    normalize_constraint_origins(&mut planned);
    assert_eq!(injected, planned);
    assert_eq!(
        injected.ids,
        vec![
            ConstraintId(0),
            ConstraintId(1),
            ConstraintId(2),
            ConstraintId(3)
        ]
    );
    assert_eq!(injected.command_results, vec![Ok(()), Ok(())]);
}

#[test]
fn manual_contention_after_reopening_follows_serial_command_order() {
    for first_is_forward in [true, false] {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");
        network.connect_bidirectional(a, b, 3);
        network.connect_bidirectional(b, c, 3);
        let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
        let forward_train = simulation
            .add_train(Train::new_manual(100, c, Direction::Forward, 3))
            .unwrap();
        let backward_train = simulation
            .add_train(Train::new_manual(100, c, Direction::Backward, 3))
            .unwrap();
        let constraint = simulation
            .create_constraint_at(
                OperationalConstraint::StationUnavailable { station: c },
                1,
                None,
                ConstraintOrigin::Injected,
            )
            .unwrap();
        simulation.step();
        simulation.remove_constraint(constraint).unwrap();
        let first_train = if first_is_forward {
            forward_train
        } else {
            backward_train
        };
        let second_train = if first_is_forward {
            backward_train
        } else {
            forward_train
        };
        assert_eq!(
            simulation.apply_command(TrainCommand::Accelerate {
                train_id: first_train,
            }),
            Ok(())
        );
        let before_second_command = simulation.snapshot();
        assert_eq!(
            simulation.apply_command(TrainCommand::Accelerate {
                train_id: second_train,
            }),
            Err(CommandError::Blocked)
        );
        let snapshot = simulation.snapshot();
        assert!(matches!(
            Simulation::snapshot_train_by_id(&snapshot, first_train).state,
            TrainSnapshotState::Moving { from, to, elapsed_seconds: 0, .. }
                if from == c && to == b
        ));
        assert_eq!(
            Simulation::snapshot_train_by_id(&snapshot, second_train).state,
            Simulation::snapshot_train_by_id(&before_second_command, second_train).state
        );
        assert!(matches!(
            Simulation::snapshot_train_by_id(&snapshot, second_train).state,
            TrainSnapshotState::Dwelling { station, .. } | TrainSnapshotState::Ready { station }
                if station == c
        ));
    }
}

#[test]
fn removal_does_not_override_an_existing_physical_station_claim() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");
    network.connect_bidirectional(a, b, 3);
    network.connect_bidirectional(b, c, 3);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let first_train = simulation
        .add_train(Train::new(100, a, Direction::Forward, 2))
        .unwrap();
    let second_train = simulation
        .add_train(Train::new(100, b, Direction::Forward, 2))
        .unwrap();
    let constraint = simulation
        .create_constraint_at(
            OperationalConstraint::TrackUnavailable { from: a, to: b },
            1,
            None,
            ConstraintOrigin::Planned,
        )
        .unwrap();
    simulation.step();
    simulation.remove_constraint(constraint).unwrap();

    simulation.step();
    let after_reopening = simulation.snapshot();
    assert!(after_reopening.constraints.is_empty());
    assert!(matches!(
        Simulation::snapshot_train_by_id(&after_reopening, first_train).state,
        TrainSnapshotState::Ready { station } if station == a
    ));
    assert!(matches!(
        Simulation::snapshot_train_by_id(&after_reopening, second_train).state,
        TrainSnapshotState::Moving { from, to, elapsed_seconds: 0, .. }
            if from == b && to == c
    ));

    simulation.step();
    assert!(matches!(
        Simulation::snapshot_train_by_id(&simulation.snapshot(), first_train).state,
        TrainSnapshotState::Moving { from, to, elapsed_seconds: 0, .. }
            if from == a && to == b
    ));
}

#[test]
fn public_scheduling_apis_have_operational_effect() {
    for relative in [false, true] {
        for manual in [false, true] {
            let mut network = Network::new();
            let a = network.add_station("A");
            let b = network.add_station("B");
            network.connect_bidirectional(a, b, 3);
            let constructor = if manual {
                Train::new_manual
            } else {
                Train::new
            };
            let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
            let train_id = sim
                .add_train(constructor(100, a, Direction::Forward, 2))
                .unwrap();
            let value = OperationalConstraint::TrackUnavailable { from: a, to: b };
            let id: ConstraintId = if relative {
                sim.create_constraint_in(value, 1, None, ConstraintOrigin::Injected)
            } else {
                sim.create_constraint_at(value, 1, None, ConstraintOrigin::Planned)
            }
            .unwrap();
            sim.step();
            let accelerate = TrainCommand::Accelerate { train_id };
            if manual {
                let before = sim.snapshot();
                assert_eq!(sim.apply_command(accelerate), Err(CommandError::Blocked));
                assert_eq!(sim.snapshot(), before);
            }
            sim.step();
            assert_eq!(
                Simulation::snapshot_train_by_id(&sim.snapshot(), train_id).state,
                TrainSnapshotState::Ready { station: a }
            );
            sim.remove_constraint(id).unwrap();
            assert_eq!(
                sim.remove_constraint(id),
                Err(ConstraintError::UnknownConstraint)
            );
            assert_eq!(
                Simulation::snapshot_train_by_id(&sim.snapshot(), train_id).state,
                TrainSnapshotState::Ready { station: a }
            );
            if manual {
                sim.apply_command(TrainCommand::Accelerate { train_id })
                    .unwrap();
            } else {
                sim.step();
            }
            assert_eq!(
                Simulation::snapshot_train_by_id(&sim.snapshot(), train_id).state,
                TrainSnapshotState::Moving {
                    from: a,
                    to: b,
                    elapsed_seconds: 0,
                    travel_seconds: 3
                }
            );
        }
    }
}

#[test]
fn public_expiry_trace_requires_a_later_departure_attempt() {
    for manual in [false, true] {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        network.connect_bidirectional(a, b, 3);
        let constructor = if manual {
            Train::new_manual
        } else {
            Train::new
        };
        let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
        let train_id = sim
            .add_train(constructor(100, a, Direction::Forward, 12))
            .unwrap();
        for _ in 0..10 {
            sim.step();
        }
        sim.create_constraint_at(
            OperationalConstraint::StationDeparturesBlocked { station: a },
            11,
            Some(15),
            ConstraintOrigin::Planned,
        )
        .unwrap();
        sim.step();
        for time in 11..15 {
            assert_eq!(sim.elapsed_seconds, time);
            if manual {
                assert_eq!(
                    sim.apply_command(TrainCommand::Accelerate { train_id }),
                    Err(CommandError::Blocked)
                );
            }
            sim.step();
            assert_eq!(
                Simulation::snapshot_train_by_id(&sim.snapshot(), train_id).state,
                TrainSnapshotState::Ready { station: a }
            );
        }
        assert_eq!(sim.elapsed_seconds, 15);
        if manual {
            sim.step();
            assert_eq!(
                Simulation::snapshot_train_by_id(&sim.snapshot(), train_id).state,
                TrainSnapshotState::Ready { station: a }
            );
            sim.apply_command(TrainCommand::Accelerate { train_id })
                .unwrap();
        } else {
            sim.step();
        }
        assert!(matches!(
            Simulation::snapshot_train_by_id(&sim.snapshot(), train_id).state,
            TrainSnapshotState::Moving {
                elapsed_seconds: 0,
                ..
            }
        ));
    }
}
