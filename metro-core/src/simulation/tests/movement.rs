use super::*;

#[test]
fn moving_train_advances_traversal_and_preserves_velocity() {
    let SimulationFixture {
        network,
        stations: [station_a, station_b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 60);

    let mut train = Train::new(
        100,
        station_a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );

    train.set_moving_for_test(station_a, station_b, 1);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();

    let train = simulation.train(train_id).unwrap();

    assert_eq!(train.velocity(), 1);
    assert_eq!(
        train.state(),
        TrainState::Moving {
            from: station_a,
            to: station_b,
            elapsed_seconds: 2,
        }
    );
}

#[test]
fn moving_train_advances_from_zero_elapsed_seconds() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 3);

    let mut train = Train::new(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    train.set_moving_for_test(a, b, 0);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 1,
        }
    );
}

#[test]
fn arrival_stops_train_and_starts_fresh_dwell() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 1);

    let mut train = Train::new(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    train.set_moving_for_test(a, b, 0);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();

    let train = simulation.train(train_id).unwrap();
    assert_eq!(train.velocity(), 0);
    assert_eq!(
        train.state(),
        TrainState::AtStation {
            station: b,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3,
            },
        }
    );
}

#[test]
fn train_remains_moving_before_track_travel_time_is_reached() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 3);

    let mut train = Train::new(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );

    train.set_moving_for_test(a, b, 0);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();
    simulation.step();

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 2,
        }
    );
}

#[test]
fn train_arrives_when_track_travel_time_is_reached() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 3);

    let mut train = Train::new(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );

    let expected_dwell_seconds = dwell_policy.dwell_seconds(b, &train);

    train.set_moving_for_test(a, b, 0);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();
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
}

#[test]
fn departure_converts_source_occupancy_into_track_and_destination_claims() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 3);

    let mut train = Train::automatic(a, F);

    train.set_moving_for_test(a, b, 0);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(Train::manual(a, F)).unwrap();

    assert_claims(&simulation.trains, &[(a, F, train_id)], &[], &[]);

    command(&mut simulation, train_id, Ok(()));

    moving(&simulation, train_id, 0, 1, 0);

    assert_claims(
        &simulation.trains,
        &[],
        &[(b, F, train_id)],
        &[(a, b, train_id)],
    );
}

#[test]
fn arrival_converts_reservation_into_destination_occupancy() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 3);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(Train::manual(a, F)).unwrap();

    command(&mut simulation, train_id, Ok(()));

    advance(&mut simulation, 3);

    arrived(&simulation, train_id, 1, F);

    assert_claims(&simulation.trains, &[(b, F, train_id)], &[], &[]);
}

#[test]
fn reservation_is_exclusive_through_traversal_without_readmission() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let moving_id = simulation.add_train(Train::manual(b, F)).unwrap();
    let blocked_id = simulation.add_train(Train::manual(b, B)).unwrap();

    command(&mut simulation, moving_id, Ok(()));

    // Reversal releases B/Forward without acquiring occupied B/Backward.
    for elapsed in 0..2 {
        moving(&simulation, moving_id, 1, 0, elapsed);

        assert_claims(
            &simulation.trains,
            &[(b, B, blocked_id)],
            &[(a, B, moving_id)],
            &[(b, a, moving_id)],
        );

        // The destination reservation remains exclusive throughout traversal.
        command(&mut simulation, blocked_id, Err(CommandError::Blocked));

        // Accelerating an already-moving manual train is an exact no-op.
        command(&mut simulation, moving_id, Ok(()));

        step(&mut simulation);
    }

    arrived(&simulation, moving_id, 0, B);

    assert_claims(
        &simulation.trains,
        &[(a, B, moving_id), (b, B, blocked_id)],
        &[],
        &[],
    );

    assert!(ResourceView::derive(simulation.trains()).track_available(b, a));

    // The track is free now; destination occupancy alone blocks departure.
    command(&mut simulation, blocked_id, Err(CommandError::Blocked));
}

#[test]
fn reverse_directed_tracks_are_independent_through_arrival() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let forward_id = simulation.add_train(Train::automatic(a, F)).unwrap();
    let backward_id = simulation.add_train(Train::automatic(b, B)).unwrap();

    advance(&mut simulation, 3);

    for elapsed in 0..2 {
        moving(&simulation, forward_id, 0, 1, elapsed);
        moving(&simulation, backward_id, 1, 0, elapsed);

        step(&mut simulation);
    }

    arrived(&simulation, forward_id, 1, F);
    arrived(&simulation, backward_id, 0, B);

    advance(&mut simulation, 3);

    // Reverse directed tracks are independent, but each train's destination
    // station slot is now occupied by the other train.
    for _ in 0..3 {
        ready(&simulation, forward_id, 1, F);
        ready(&simulation, backward_id, 0, B);

        step(&mut simulation);
    }
}
