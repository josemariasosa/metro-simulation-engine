use super::*;

#[test]
fn admitted_departure_rederives_exact_ownership_including_reversal() {
    for reverse in [false, true] {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        network.connect_bidirectional(a, b, 2);

        let (from, to, direction) = if reverse {
            (b, a, Direction::Backward)
        } else {
            (a, b, Direction::Forward)
        };

        let train = Train::new_manual(
            100,
            from,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        // A reversal must not require the opposite source slot to be free.
        let other = Train::new_manual(
            100,
            from,
            Direction::Backward,
            DwellPolicy::default_dwell_seconds(),
        );
        let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
        let train_id = simulation.add_train(train).unwrap();
        let other_id = simulation.add_train(other).unwrap();

        let before = ResourceView::derive(simulation.trains());
        let source = StationSlot {
            station: from,
            direction: Direction::Forward,
        };
        assert_eq!(before.station_occupants.get(&source), Some(&train_id));

        simulation
            .apply_command(TrainCommand::Accelerate { train_id })
            .unwrap();

        assert_eq!(simulation.elapsed_seconds, 0);
        let train = simulation.train(train_id).unwrap();

        assert_eq!(train.direction(), direction);
        assert_eq!(train.velocity(), 1);
        assert_eq!(
            train.state(),
            TrainState::Moving {
                from,
                to,
                elapsed_seconds: 0
            }
        );
        let after = ResourceView::derive(simulation.trains());
        assert!(!after.station_occupants.contains_key(&source));
        assert_eq!(
            after.station_occupants,
            HashMap::from([(
                StationSlot {
                    station: from,
                    direction: Direction::Backward
                },
                other_id
            ),])
        );
        assert_eq!(
            after.track_occupants,
            HashMap::from([((from, to), train_id)])
        );
        assert_eq!(
            after.station_reservations,
            HashMap::from([(
                StationSlot {
                    station: to,
                    direction
                },
                train_id
            ),])
        );
    }
}

#[test]
fn opposite_direction_resources_do_not_block_manual_departure() {
    for blocker_kind in 0..3 {
        let SimulationFixture {
            network,
            stations: [a, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);

        let train = Train::new_manual(
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let blocker = match blocker_kind {
            0 => Train::new_manual(
                100,
                b,
                Direction::Backward,
                DwellPolicy::default_dwell_seconds(),
            ),
            1 => Train::moving_train(c, b, Direction::Backward),
            _ => Train::moving_train(b, a, Direction::Backward),
        };

        let mut simulation = Simulation::new(network, vec![], dwell_policy);
        let train_id = simulation.add_train(train).unwrap();
        let blocker_id = simulation.add_train(blocker).unwrap();
        let before = simulation.snapshot();

        simulation
            .apply_command(TrainCommand::Accelerate { train_id })
            .unwrap();

        let after = simulation.snapshot();

        assert_eq!(
            Simulation::snapshot_train_by_id(&after, blocker_id),
            Simulation::snapshot_train_by_id(&before, blocker_id),
        );
        assert_eq!(simulation.elapsed_seconds, before.elapsed_seconds);
        assert_eq!(
            simulation.train(train_id).unwrap().state(),
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );
        assert_eq!(simulation.train(train_id).unwrap().direction(), Direction::Forward);
        assert_eq!(simulation.train(train_id).unwrap().velocity(), 1);
        ResourceView::derive(simulation.trains());
    }
}

#[test]
fn departure_candidate_is_pure_until_committed() {
    let SimulationFixture {
        network,
        stations: [_, b, c],
        ..
    } = SimulationFixture::new(["A", "B", "C"], 10);
    let mut train = Train::new_manual(
        100,
        c,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    let before = train.clone();
    let TrainState::AtStation { station, .. } = train.state() else {
        panic!("expected station state");
    };
    let candidate = select_departure_candidate(&network, station, train.direction()).unwrap();

    assert_eq!(candidate.from, c);
    assert_eq!(candidate.to, b);
    assert_eq!(candidate.direction, Direction::Backward);
    assert_eq!(train.capacity(), before.capacity());
    assert_eq!(train.is_manual_control(), before.is_manual_control());
    assert_eq!(train.is_automatic_control(), before.is_automatic_control());
    assert_eq!(train.state(), before.state());
    assert_eq!(train.direction(), before.direction());
    assert_eq!(train.velocity(), before.velocity());

    train.apply_departure(candidate.from, candidate.to, candidate.direction);

    assert_eq!(train.capacity(), before.capacity());
    assert_eq!(train.is_manual_control(), before.is_manual_control());
    assert_eq!(train.is_automatic_control(), before.is_automatic_control());
    assert_eq!(train.direction(), Direction::Backward);
    assert_eq!(train.velocity(), 1);
    assert_eq!(
        train.state(),
        TrainState::Moving {
            from: c,
            to: b,
            elapsed_seconds: 0
        }
    );
}

#[test]
fn automatic_departure_prefers_current_direction_then_reverse_fallback() {
    for (station, direction, gap, to, selected_direction) in [
        (1, Direction::Forward, false, 2, Direction::Forward),
        (1, Direction::Backward, false, 0, Direction::Backward),
        (2, Direction::Forward, false, 1, Direction::Backward),
        (0, Direction::Backward, false, 1, Direction::Forward),
        (1, Direction::Forward, true, 0, Direction::Backward),
    ] {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");
        network.connect_bidirectional(a, b, 10);
        if !gap {
            network.connect_bidirectional(b, c, 10);
        }
        let train = Train::new(
            100,
            StationId(station),
            direction,
            DwellPolicy::default_dwell_seconds(),
        );
        let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
        let train_id = simulation.add_train(train).unwrap();

        for _ in 0..3 {
            simulation.step();
        }

        let train = simulation.train(train_id).unwrap();
        assert_eq!(simulation.elapsed_seconds, 3);
        assert_eq!(train.direction(), selected_direction);
        assert_eq!(train.velocity(), 1);
        assert_eq!(
            train.state(),
            TrainState::Moving {
                from: StationId(station),
                to: StationId(to),
                elapsed_seconds: 0,
            }
        );
    }
}

#[test]
#[should_panic(expected = "NO_NEXT_TRACK_AFTER_REVERSING_DIRECTION")]
fn automatic_departure_panics_when_station_has_no_outgoing_track() {
    let mut network = Network::new();
    let station = network.add_station("Isolated");
    let train = Train::new(
        100,
        station,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());
    for _ in 0..3 {
        simulation.step();
    }
}

#[test]
fn automatic_ready_train_retries_departure_on_next_step() {
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

    train.set_ready_for_test(a);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();

    assert_eq!(simulation.elapsed_seconds, 1);
    assert_eq!(simulation.train(train_id).unwrap().velocity(), 1);

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0,
        }
    );
}

#[test]
fn automatic_departure_starts_with_velocity_one() {
    let SimulationFixture {
        network,
        stations: [station_a, station_b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 60);

    let train = Train::new(
        100,
        station_a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    for _ in 0..3 {
        simulation.step();
    }

    let train = simulation.train(train_id).unwrap();

    assert_eq!(train.velocity(), 1);
    assert_eq!(
        train.state(),
        TrainState::Moving {
            from: station_a,
            to: station_b,
            elapsed_seconds: 0,
        }
    );
}

#[test]
fn automatic_departure_reverses_at_end_of_line() {
    let SimulationFixture {
        network,
        stations: [_, b, c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 10);

    let mut train = Train::new(
        100,
        c,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    train.set_ready_for_test(c);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    simulation.step();

    let train = simulation.train(train_id).unwrap();

    assert_eq!(train.direction(), Direction::Backward);
    assert_eq!(train.velocity(), 1);
    assert_eq!(
        train.state(),
        TrainState::Moving {
            from: c,
            to: b,
            elapsed_seconds: 0,
        }
    );
}

#[test]
fn arrival_at_terminal_keeps_direction_until_next_departure_is_committed() {
    let SimulationFixture {
        network,
        stations: [a, _b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 3);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(Train::manual(a, F)).unwrap();

    command(&mut simulation, train_id, Ok(()));
    advance(&mut simulation, 3);

    arrived(&simulation, train_id, 1, F);

    assert_eq!(simulation.train(train_id).unwrap().direction(), Direction::Forward);

    command(&mut simulation, train_id, Ok(()));

    assert_eq!(simulation.train(train_id).unwrap().direction(), Direction::Backward);

    assert_eq!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving {
            from: StationId(1),
            to: StationId(0),
            elapsed_seconds: 0,
        }
    );
}

#[test]
fn opposite_direction_arrivals_share_a_logical_station() {
    let SimulationFixture {
        network,
        stations: [a, b, c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let forward_id = simulation.add_train(Train::automatic(a, F)).unwrap();
    let backward_id = simulation.add_train(Train::automatic(c, B)).unwrap();

    advance(&mut simulation, 3);

    for elapsed in 0..2 {
        moving(&simulation, forward_id, 0, 1, elapsed);
        moving(&simulation, backward_id, 2, 1, elapsed);

        assert_claims(
            &simulation.trains,
            &[],
            &[(b, F, forward_id), (b, B, backward_id)],
            &[(a, b, forward_id), (c, b, backward_id)],
        );

        step(&mut simulation);
    }

    arrived(&simulation, forward_id, 1, F);
    arrived(&simulation, backward_id, 1, B);
}
