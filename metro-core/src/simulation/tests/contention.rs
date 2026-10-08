use super::*;

fn line<const N: usize>(names: [&str; N]) -> (Network, [StationId; N]) {
    let mut network = Network::new();
    let stations = names.map(|name| network.add_station(name));
    for pair in stations.windows(2) {
        network.connect_bidirectional(pair[0], pair[1], 2);
    }
    (network, stations)
}

fn ready_snapshot(id: TrainId, station: StationId, direction: Direction) -> TrainSnapshot {
    TrainSnapshot {
        id,
        direction,
        velocity: 0,
        state: TrainSnapshotState::Ready { station },
    }
}

fn moving_snapshot(
    id: TrainId,
    from: StationId,
    to: StationId,
    direction: Direction,
    elapsed_seconds: u64,
) -> TrainSnapshot {
    TrainSnapshot {
        id,
        direction,
        velocity: 1,
        state: TrainSnapshotState::Moving {
            from,
            to,
            elapsed_seconds,
            travel_seconds: 2,
        },
    }
}

fn by_id(sim: &Simulation) -> HashMap<TrainId, TrainSnapshot> {
    sim.snapshot()
        .trains
        .into_iter()
        .map(|train| (train.id, train))
        .collect()
}

fn assert_order(sim: &Simulation, ids: &[TrainId]) {
    assert_eq!(sim.trains().map(|(id, _)| id).collect::<Vec<_>>(), ids);
    assert_eq!(
        sim.snapshot()
            .trains
            .iter()
            .map(|train| train.id)
            .collect::<Vec<_>>(),
        ids
    );
}

#[test]
fn conga_freezes_starting_ownership_in_both_storage_orders() {
    let mut traces = Vec::new();
    for reversed in [false, true] {
        let (network, [a, b, c, d]) = line(["A", "B", "C", "D"]);
        let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
        let ids = [
            sim.add_train(Train::new(100, a, Direction::Forward, 3))
                .unwrap(),
            sim.add_train(Train::new(100, b, Direction::Forward, 3))
                .unwrap(),
            sim.add_train(Train::new(100, c, Direction::Forward, 3))
                .unwrap(),
        ];
        if reversed {
            sim.reverse_train_order_for_test();
        }
        let order = sim.trains().map(|(id, _)| id).collect::<Vec<_>>();
        let mut trace = Vec::new();
        let expected = [
            vec![
                ready_snapshot(ids[0], a, Direction::Forward),
                ready_snapshot(ids[1], b, Direction::Forward),
                moving_snapshot(ids[2], c, d, Direction::Forward, 0),
            ],
            vec![
                ready_snapshot(ids[0], a, Direction::Forward),
                moving_snapshot(ids[1], b, c, Direction::Forward, 0),
                moving_snapshot(ids[2], c, d, Direction::Forward, 1),
            ],
            vec![
                moving_snapshot(ids[0], a, b, Direction::Forward, 0),
                moving_snapshot(ids[1], b, c, Direction::Forward, 1),
                TrainSnapshot {
                    id: ids[2],
                    direction: Direction::Forward,
                    velocity: 0,
                    state: TrainSnapshotState::Dwelling {
                        station: d,
                        remaining_seconds: 3,
                    },
                },
            ],
        ];
        for time in 1..=5 {
            sim.step();
            assert_eq!(sim.snapshot().elapsed_seconds, time);
            assert_order(&sim, &order);
            if time >= 3 {
                // Sequential mutation in reverse storage order could release C,
                // then B, and incorrectly let all three depart at t=3. Each
                // departure must instead wait for the next committed world.
                let actual = by_id(&sim);
                let wanted = expected[(time - 3) as usize]
                    .iter()
                    .cloned()
                    .map(|train| (train.id, train))
                    .collect::<HashMap<_, _>>();
                assert_eq!(actual, wanted, "conga t={time}, reversed={reversed}");
                trace.push(actual);
            }
        }
        let t3 = sim.trains().find(|(id, _)| *id == ids[2]).unwrap();
        assert_eq!(
            t3.1.state(),
            TrainState::AtStation {
                station: d,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: 3
                },
            }
        );
        traces.push(trace);
    }
    assert_eq!(traces[0], traces[1]);
}

#[test]
fn terminal_contention_uses_numeric_id_in_both_storage_orders() {
    let mut outcomes = Vec::new();
    for reversed in [false, true] {
        let (network, [a, b]) = line(["A", "B"]);
        let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
        let first_id = sim
            .add_train(Train::new(100, b, Direction::Forward, 3))
            .unwrap();
        let second_id = sim
            .add_train(Train::new(100, b, Direction::Backward, 3))
            .unwrap();
        assert!(first_id.0 < second_id.0);
        if reversed {
            sim.reverse_train_order_for_test();
        }
        let order = sim.trains().map(|(id, _)| id).collect::<Vec<_>>();
        for _ in 0..3 {
            sim.step();
            assert_order(&sim, &order);
        }
        assert_eq!(sim.elapsed_seconds, 3);
        let outcome = by_id(&sim);
        assert_eq!(
            outcome[&first_id],
            moving_snapshot(first_id, b, a, Direction::Backward, 0)
        );
        assert_eq!(
            outcome[&second_id],
            ready_snapshot(second_id, b, Direction::Backward)
        );
        outcomes.push(outcome);

        advance(&mut sim, 2);
        arrived(&sim, first_id, 0, Direction::Backward);

        advance(&mut sim, 3);
        moving(&sim, first_id, 0, 1, 0);
        assert_eq!(sim.train(first_id).unwrap().direction(), Direction::Forward);
        ready(&sim, second_id, 1, Direction::Backward);

        sim.step();
        moving(&sim, second_id, 1, 0, 0);
        assert_eq!(sim.train(second_id).unwrap().direction(), Direction::Backward);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn serial_manual_contention_is_won_by_call_order_not_id() {
    for first_wins in [true, false] {
        let SimulationFixture {
            network,
            stations: [_a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 2);

        let mut simulation = Simulation::new(network, vec![], dwell_policy);

        let first_id = simulation.add_train(Train::manual(b, F)).unwrap();
        let second_id = simulation.add_train(Train::manual(b, B)).unwrap();

        let (winner, loser) = if first_wins {
            (first_id, second_id)
        } else {
            (second_id, first_id)
        };

        command(&mut simulation, winner, Ok(()));
        command(&mut simulation, loser, Err(CommandError::Blocked));

        moving(&simulation, winner, 1, 0, 0);

        advance(&mut simulation, 2);

        arrived(&simulation, winner, 0, B);

        command(&mut simulation, loser, Err(CommandError::Blocked));
    }
}

#[test]
fn manual_command_sees_resources_released_by_completed_automatic_step() {
    let SimulationFixture {
        network,
        stations: [a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let first_automatic = simulation.add_train(Train::automatic(a, F)).unwrap();

    let second_automatic = simulation.add_train(Train::automatic(b, F)).unwrap();

    let manual = simulation.add_train(Train::manual(a, B)).unwrap();

    advance(&mut simulation, 3);

    // Frozen World N prevents the first automatic train from using B
    // during the same automatic departure step.
    ready(&simulation, first_automatic, 0, F);
    moving(&simulation, second_automatic, 1, 2, 0);
    ready(&simulation, manual, 0, B);

    command(&mut simulation, manual, Ok(()));

    assert_eq!(simulation.elapsed_seconds, 3);

    moving(&simulation, manual, 0, 1, 0);
    ready(&simulation, first_automatic, 0, F);

    advance(&mut simulation, 2);

    arrived(&simulation, manual, 1, F);
    arrived(&simulation, second_automatic, 2, F);
    ready(&simulation, first_automatic, 0, F);
}

#[test]
fn occupied_slot_retry_and_manual_no_buffering_preserve_invariants() {
    for is_manual in [false, true] {
        let SimulationFixture {
            network,
            stations: [a, b, _],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);

        let mut simulation = Simulation::new(network, vec![], dwell_policy);

        let follower_id = simulation
            .add_train(if is_manual {
                Train::manual(a, F)
            } else {
                Train::automatic(a, F)
            })
            .unwrap();

        let blocker_id = simulation.add_train(Train::manual(b, F)).unwrap();

        if is_manual {
            command(&mut simulation, follower_id, Err(CommandError::Blocked));
        }

        advance(&mut simulation, 3);

        ready(&simulation, follower_id, 0, F);

        if is_manual {
            command(&mut simulation, follower_id, Err(CommandError::Blocked));
        }

        step(&mut simulation);

        ready(&simulation, follower_id, 0, F);

        command(&mut simulation, blocker_id, Ok(()));

        step(&mut simulation);

        if is_manual {
            // Rejected manual intent was not buffered; a fresh command is required.
            ready(&simulation, follower_id, 0, F);

            command(&mut simulation, follower_id, Ok(()));
        }

        moving(&simulation, follower_id, 0, 1, 0);

        advance(&mut simulation, 2);

        arrived(&simulation, follower_id, 1, F);
    }
}

#[test]
fn blocked_reversal_retains_direction_until_accepted_departure() {
    for is_manual in [false, true] {
        let SimulationFixture {
            network,
            stations: [_, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);

        let mut simulation = Simulation::new(network, vec![], dwell_policy);

        let follower_id = simulation
            .add_train(if is_manual {
                Train::manual(c, F)
            } else {
                Train::automatic(c, F)
            })
            .unwrap();

        let blocker_id = simulation.add_train(Train::manual(b, B)).unwrap();

        if is_manual {
            command(&mut simulation, follower_id, Err(CommandError::Blocked));
        }

        advance(&mut simulation, 3);

        ready(&simulation, follower_id, 2, F);

        if is_manual {
            command(&mut simulation, follower_id, Err(CommandError::Blocked));
        }

        step(&mut simulation);

        ready(&simulation, follower_id, 2, F);

        command(&mut simulation, blocker_id, Ok(()));

        step(&mut simulation);

        if is_manual {
            ready(&simulation, follower_id, 2, F);

            command(&mut simulation, follower_id, Ok(()));
        }

        moving(&simulation, follower_id, 2, 1, 0);

        advance(&mut simulation, 2);

        arrived(&simulation, follower_id, 1, B);
    }
}
