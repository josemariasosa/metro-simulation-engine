use metro_core::DwellPolicy;
use metro_core::Network;
use metro_core::StationId;
use metro_core::simulation::Simulation;
use metro_core::{AtStationState, Direction, Train, TrainId, TrainState};

fn build_tiny_metro_network() -> (Network, [StationId; 5]) {
    let mut network = Network::new();

    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");
    let d = network.add_station("D");
    let e = network.add_station("E");

    network.connect_bidirectional(a, b, 180);
    network.connect_bidirectional(b, c, 120);
    network.connect_bidirectional(c, d, 240);
    network.connect_bidirectional(d, e, 180);

    (network, [a, b, c, d, e])
}

#[test]
fn scenario_topology_is_five_stations_with_expected_segment_travel_times() {
    let (network, [a, b, c, d, e]) = build_tiny_metro_network();

    assert_eq!(network.station_count(), 5);
    assert_eq!(network.track(a, b).unwrap().travel_seconds, 180);
    assert_eq!(network.track(b, c).unwrap().travel_seconds, 120);
    assert_eq!(network.track(c, d).unwrap().travel_seconds, 240);
    assert_eq!(network.track(d, e).unwrap().travel_seconds, 180);

    assert_eq!(network.track(b, a).unwrap().travel_seconds, 180);
    assert_eq!(network.track(c, b).unwrap().travel_seconds, 120);
    assert_eq!(network.track(d, c).unwrap().travel_seconds, 240);
    assert_eq!(network.track(e, d).unwrap().travel_seconds, 180);
}

fn assert_dwelling_at(simulation: &Simulation, train_id: TrainId, station: StationId) {
    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::AtStation {
            station,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3,
            },
        }
    );
}

fn assert_moving(
    simulation: &Simulation,
    train_id: TrainId,
    from: StationId,
    to: StationId,
    elapsed_seconds: u64,
) {
    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::Moving {
            from,
            to,
            elapsed_seconds,
        }
    );
    assert_eq!(simulation.train(train_id).velocity(), 1);
}

#[test]
fn two_trains_traverse_the_line_reverse_and_return_toward_origin() {
    let (network, [a, b, c, d, e]) = build_tiny_metro_network();
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
    let forward_train = simulation
        .add_train(Train::new(100, a, Direction::Forward, 3))
        .unwrap();
    let backward_train = simulation
        .add_train(Train::new(100, e, Direction::Backward, 3))
        .unwrap();

    while simulation.elapsed_seconds < 183 {
        simulation.step();
    }
    assert_eq!(simulation.elapsed_seconds, 183);
    assert_dwelling_at(&simulation, forward_train, b);
    assert_dwelling_at(&simulation, backward_train, d);
    assert_eq!(simulation.train(forward_train).velocity(), 0);
    assert_eq!(simulation.train(backward_train).velocity(), 0);

    while simulation.elapsed_seconds < 306 {
        simulation.step();
    }
    assert_dwelling_at(&simulation, forward_train, c);
    assert_moving(&simulation, backward_train, d, c, 120);

    while simulation.elapsed_seconds < 549 {
        simulation.step();
    }
    assert_dwelling_at(&simulation, forward_train, d);
    assert_dwelling_at(&simulation, backward_train, b);

    while simulation.elapsed_seconds < 732 {
        simulation.step();
    }
    assert_dwelling_at(&simulation, forward_train, e);
    assert_dwelling_at(&simulation, backward_train, a);

    simulation.step();
    assert_eq!(simulation.elapsed_seconds, 733);
    for (train_id, station) in [(forward_train, e), (backward_train, a)] {
        assert_eq!(
            simulation.train(train_id).state(),
            TrainState::AtStation {
                station,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 1,
                    dwell_seconds: 3,
                },
            }
        );
    }

    simulation.step();
    simulation.step();
    assert_eq!(simulation.elapsed_seconds, 735);
    assert_moving(&simulation, forward_train, e, d, 0);
    assert_moving(&simulation, backward_train, a, b, 0);
    assert_eq!(
        simulation.train(forward_train).direction(),
        Direction::Backward
    );
    assert_eq!(
        simulation.train(backward_train).direction(),
        Direction::Forward
    );

    simulation.step();
    assert_eq!(simulation.elapsed_seconds, 736);
    assert_moving(&simulation, forward_train, e, d, 1);
    assert_moving(&simulation, backward_train, a, b, 1);
}
