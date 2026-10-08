use super::*;

#[test]
fn train_keeps_reversed_direction_after_intermediate_stop() {
    let SimulationFixture {
        network,
        stations: [a, b, c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let train = Train::new(
        100,
        c,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    let expected_dwell_seconds = dwell_policy.dwell_seconds(b, &train);
    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    // Dwell at C, then reverse and depart toward B.
    simulation.step();
    simulation.step();
    simulation.step();

    assert_eq!(simulation.train(train_id).unwrap().direction(), Direction::Backward);

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: c,
            to: b,
            elapsed_seconds: 0,
        }
    );

    // Travel C -> B.
    simulation.step();
    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::AtStation {
            station: b,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: expected_dwell_seconds,
            },
        }
    );

    // Dwell at B, then continue toward A.
    simulation.step();
    simulation.step();
    simulation.step();

    assert_eq!(simulation.train(train_id).unwrap().direction(), Direction::Backward);

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: b,
            to: a,
            elapsed_seconds: 0,
        }
    );
}
