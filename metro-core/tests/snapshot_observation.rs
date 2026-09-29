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

fn expected_snapshot(elapsed_seconds: u64, state: TrainSnapshotState) -> SimulationSnapshot {
    SimulationSnapshot {
        elapsed_seconds,
        trains: vec![TrainSnapshot {
            id: TrainId(0),
            direction: Direction::Forward,
            state,
        }],
    }
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
