use metro_core::dwell::DwellPolicy;
use metro_core::network::Network;
use metro_core::simulation::Simulation;
use metro_core::station::StationId;
use metro_core::train::{AtStationState, Direction, Train, TrainId, TrainState};

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

fn assert_train_dwelling_at(train_state: &TrainState, test_station: &StationId) {
    match train_state {
        TrainState::AtStation { station, state } => {
            assert_eq!(*station, *test_station);

            // ahora comprobar que `state`
            // es AtStationState::Dwelling { ... }
            match state {
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                } => {
                    assert_eq!(*elapsed_seconds, 0);
                    assert_eq!(*dwell_seconds, 3);
                }
            }
        }
        _ => panic!("train should start at station {:?}", test_station),
    }
}

#[test]
fn trains_start_at_opposite_endpoints_in_opposite_directions() {
    let (network, [a, _b, _c, _d, e]) = build_tiny_metro_network();
    let dwell_policy = DwellPolicy::new();

    let trains = vec![
        Train::new(TrainId(0), 100, a, Direction::Forward),
        Train::new(TrainId(1), 100, e, Direction::Backward),
    ];

    let simulation = Simulation::new(network, trains, dwell_policy);

    assert_eq!(simulation.trains().len(), 2);
    assert_eq!(simulation.trains()[0].id, TrainId(0));
    assert_eq!(simulation.trains()[1].id, TrainId(1));

    assert_eq!(simulation.trains()[0].direction, Direction::Forward);
    assert_train_dwelling_at(&simulation.trains()[0].state, &a);

    assert_eq!(simulation.trains()[1].direction, Direction::Backward);
    assert_train_dwelling_at(&simulation.trains()[1].state, &e);
}

#[test]
fn both_trains_depart_after_dwell() {
    let (network, [a, b, _c, d, e]) = build_tiny_metro_network();
    let dwell_policy = DwellPolicy::new();

    let trains = vec![
        Train::new(TrainId(0), 100, a, Direction::Forward),
        Train::new(TrainId(1), 100, e, Direction::Backward),
    ];

    let mut simulation = Simulation::new(network, trains, dwell_policy);

    simulation.step(); // advance the simulation by one step

    // assert that both trains have departed from their starting stations
    assert_eq!(
        simulation.trains()[0].state,
        TrainState::AtStation {
            station: a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 1,
                dwell_seconds: 3
            }
        }
    );
    assert_eq!(
        simulation.trains()[1].state,
        TrainState::AtStation {
            station: e,
            state: AtStationState::Dwelling {
                elapsed_seconds: 1,
                dwell_seconds: 3
            }
        }
    );

    simulation.step(); // advance the simulation by one step

    assert_eq!(
        simulation.trains()[0].state,
        TrainState::AtStation {
            station: a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 2,
                dwell_seconds: 3
            }
        }
    );
    assert_eq!(
        simulation.trains()[1].state,
        TrainState::AtStation {
            station: e,
            state: AtStationState::Dwelling {
                elapsed_seconds: 2,
                dwell_seconds: 3
            }
        }
    );

    simulation.step(); // advance the simulation by one step

    assert_eq!(
        simulation.trains()[0].state,
        TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0
        }
    );
    assert_eq!(
        simulation.trains()[1].state,
        TrainState::Moving {
            from: e,
            to: d,
            elapsed_seconds: 0
        }
    );
}

#[test]
// t=0
// │
// │ 732 steps
// ▼
// t=732   ✓ ambos llegaron al extremo opuesto
// │
// │ 3 steps
// ▼
// t=735   ✓ reversal + comienzan track de regreso
// │
// │ 1 step
// ▼
// t=736   ✓ efectivamente están avanzando de regreso
fn both_trains_traverse_line_reverse_and_head_back_toward_origin() {
    let (network, [a, b, _c, d, e]) = build_tiny_metro_network();
    let dwell_policy = DwellPolicy::new();

    let trains = vec![
        Train::new(TrainId(0), 100, a, Direction::Forward),
        Train::new(TrainId(1), 100, e, Direction::Backward),
    ];

    let mut simulation = Simulation::new(network, trains, dwell_policy);

    for _ in 0..732 {
        simulation.step();
    }

    // assert that both trains have reached the opposite ends
    assert_eq!(
        simulation.trains()[0].state,
        TrainState::AtStation {
            station: e,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3
            }
        }
    );
    assert_eq!(
        simulation.trains()[1].state,
        TrainState::AtStation {
            station: a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3
            }
        }
    );

    for _ in 0..3 {
        simulation.step();
    }

    // assert that both trains have reversed and started heading back
    assert_eq!(
        simulation.trains()[0].state,
        TrainState::Moving {
            from: e,
            to: d,
            elapsed_seconds: 0
        }
    );
    assert_eq!(
        simulation.trains()[1].state,
        TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0
        }
    );

    simulation.step();

    // assert that both trains are effectively moving back toward the origin
    assert_eq!(
        simulation.trains()[0].state,
        TrainState::Moving {
            from: e,
            to: d,
            elapsed_seconds: 1
        }
    );
    assert_eq!(
        simulation.trains()[1].state,
        TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 1
        }
    );

    assert_eq!(simulation.trains()[0].direction, Direction::Backward);
    assert_eq!(simulation.trains()[1].direction, Direction::Forward);
}
