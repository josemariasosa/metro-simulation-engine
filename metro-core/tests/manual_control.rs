use metro_core::dwell::DwellPolicy;
use metro_core::network::Network;
use metro_core::simulation::Simulation;
use metro_core::snapshot::TrainSnapshotState;
use metro_core::train::{AtStationState, Direction, Train, TrainId, TrainState};

#[test]
fn manual_train_becomes_ready_after_dwell_completes() {
    let mut network = Network::new();
    let station_a = network.add_station("A");
    let train = Train::new_manual(TrainId(0), 100, station_a, Direction::Forward);
    let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());

    for elapsed_seconds in 0..3 {
        assert_eq!(simulation.elapsed_seconds, elapsed_seconds);
        assert_eq!(
            simulation.trains()[0].state,
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
        simulation.trains()[0].state,
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
    let train = Train::new_manual(TrainId(0), 100, a, Direction::Forward);
    let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());

    simulation.step();
    simulation.step();
    simulation.step();

    let ready_state = simulation.trains()[0].state;
    let direction = simulation.trains()[0].direction;
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
        assert_eq!(simulation.trains()[0].state, ready_state);
        assert_eq!(simulation.trains()[0].direction, direction);
        assert_eq!(simulation.snapshot().trains[0].velocity, 0);
    }
}

#[test]
fn manual_train_waits_ready_at_endpoint_without_reversing() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.connect_bidirectional(a, b, 10);
    let train = Train::new_manual(TrainId(0), 100, b, Direction::Forward);
    let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());

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
        assert_eq!(simulation.trains()[0].direction, Direction::Forward);
        assert_eq!(
            simulation.trains()[0].state,
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
