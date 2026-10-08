use crate::{
    AtStationState, Direction, DwellPolicy, Train, TrainState,
    simulation::{Simulation, tests::SimulationFixture},
};

#[test]
fn new_train_starts_dwelling_and_step_advances_dwell_time() {
    let SimulationFixture {
        network,
        stations: [station_a, _],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 60);

    let train = Train::new(
        100,
        station_a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );

    let mut simulation = Simulation::new(network, vec![], dwell_policy.clone());
    let train_id = simulation.add_train(train.clone()).unwrap();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::AtStation {
            station: station_a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
            }
        }
    );

    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::AtStation {
            station: station_a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 1,
                dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
            }
        }
    );
}

#[test]
fn train_departs_when_dwell_time_is_reached() {
    let SimulationFixture {
        network,
        stations: [station_a, station_b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 10);

    let train = Train::new(
        100,
        station_a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    let mut simulation = Simulation::new(network, vec![], dwell_policy.clone());
    let train_id = simulation.add_train(train.clone()).unwrap();

    simulation.step();
    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::AtStation {
            station: station_a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 2,
                dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
            },
        }
    );

    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: station_a,
            to: station_b,
            elapsed_seconds: 0,
        }
    );
}

#[test]
fn configured_dwell_duration_is_preserved_until_departure() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 10);

    let mut train = Train::new(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    train.set_dwell_for_test(a, 5);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();
    simulation.step();
    simulation.step();
    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::AtStation {
            station: a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 4,
                dwell_seconds: 5,
            },
        }
    );

    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0,
        }
    );
}
