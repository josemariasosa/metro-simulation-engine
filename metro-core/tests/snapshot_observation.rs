use metro_core::dwell::DwellPolicy;
use metro_core::network::Network;
use metro_core::simulation::Simulation;
use metro_core::snapshot::{SimulationSnapshot, TrainSnapshot, TrainSnapshotState};
use metro_core::station::StationId;
use metro_core::train::{Direction, Train, TrainId};

fn test_simulation(travel_seconds: u64) -> Simulation {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, travel_seconds);
    let train = Train::new(TrainId(0), 100, a, Direction::Forward);
    Simulation::new(network, vec![train], DwellPolicy::new())
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
        Train::new(TrainId(9), 100, b, Direction::Backward),
        Train::new(TrainId(2), 100, a, Direction::Forward),
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
                    id: TrainId(9),
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
                    id: TrainId(2),
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
        }
    );
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
