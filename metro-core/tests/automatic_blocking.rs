use std::collections::BTreeMap;

use metro_core::command::TrainCommand;
use metro_core::dwell::DwellPolicy;
use metro_core::network::Network;
use metro_core::simulation::Simulation;
use metro_core::snapshot::{TrainSnapshot, TrainSnapshotState};
use metro_core::station::StationId;
use metro_core::train::{AtStationState, Direction, Train, TrainId, TrainState};

fn line<const N: usize>(names: [&str; N]) -> (Network, [StationId; N]) {
    let mut network = Network::new();
    let stations = names.map(|name| network.add_station(name));
    for pair in stations.windows(2) {
        network.connect_bidirectional(pair[0], pair[1], 2);
    }
    (network, stations)
}

fn ready(id: usize, station: StationId, direction: Direction) -> TrainSnapshot {
    TrainSnapshot {
        id: TrainId(id),
        direction,
        velocity: 0,
        state: TrainSnapshotState::Ready { station },
    }
}

fn moving(
    id: usize,
    from: StationId,
    to: StationId,
    direction: Direction,
    elapsed_seconds: u64,
) -> TrainSnapshot {
    TrainSnapshot {
        id: TrainId(id),
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

fn by_id(sim: &Simulation) -> BTreeMap<usize, TrainSnapshot> {
    sim.snapshot()
        .trains
        .into_iter()
        .map(|train| (train.id.0, train))
        .collect()
}

fn assert_order(sim: &Simulation, ids: &[usize]) {
    assert_eq!(
        sim.trains()
            .iter()
            .map(|train| train.id.0)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        sim.snapshot()
            .trains
            .iter()
            .map(|train| train.id.0)
            .collect::<Vec<_>>(),
        ids
    );
}

#[test]
fn conga_freezes_starting_ownership_in_both_storage_orders() {
    let mut traces = Vec::new();
    for reversed in [false, true] {
        let (network, [a, b, c, d]) = line(["A", "B", "C", "D"]);
        let mut trains = vec![
            Train::new(TrainId(1), 100, a, Direction::Forward, 3),
            Train::new(TrainId(2), 100, b, Direction::Forward, 3),
            Train::new(TrainId(3), 100, c, Direction::Forward, 3),
        ];
        if reversed {
            trains.reverse();
        }
        let order = trains.iter().map(|train| train.id.0).collect::<Vec<_>>();
        let mut sim = Simulation::new(network, trains, DwellPolicy::new());
        let mut trace = Vec::new();
        let expected = [
            vec![
                ready(1, a, Direction::Forward),
                ready(2, b, Direction::Forward),
                moving(3, c, d, Direction::Forward, 0),
            ],
            vec![
                ready(1, a, Direction::Forward),
                moving(2, b, c, Direction::Forward, 0),
                moving(3, c, d, Direction::Forward, 1),
            ],
            vec![
                moving(1, a, b, Direction::Forward, 0),
                moving(2, b, c, Direction::Forward, 1),
                TrainSnapshot {
                    id: TrainId(3),
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
                    .map(|train| (train.id.0, train))
                    .collect::<BTreeMap<_, _>>();
                assert_eq!(actual, wanted, "conga t={time}, reversed={reversed}");
                trace.push(actual);
            }
        }
        let t3 = sim
            .trains()
            .iter()
            .find(|train| train.id == TrainId(3))
            .unwrap();
        assert_eq!(
            t3.state,
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
fn blocked_automatic_retries_without_restarting_dwell() {
    for extra_blocked_steps in [0, 3] {
        let (network, [a, b, c]) = line(["A", "B", "C"]);
        let mut sim = Simulation::new(
            network,
            vec![
                Train::new(TrainId(1), 100, a, Direction::Forward, 3),
                Train::new_manual(TrainId(2), 100, b, Direction::Forward, 3),
            ],
            DwellPolicy::new(),
        );
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(by_id(&sim)[&1], ready(1, a, Direction::Forward));
        for _ in 0..extra_blocked_steps {
            sim.step();
            assert_eq!(by_id(&sim)[&1], ready(1, a, Direction::Forward));
        }
        sim.apply_command(TrainCommand::Accelerate {
            train_id: TrainId(2),
        })
        .unwrap();
        assert_eq!(by_id(&sim)[&2], moving(2, b, c, Direction::Forward, 0));
        sim.step();
        assert_eq!(sim.elapsed_seconds, 4 + extra_blocked_steps);
        assert_eq!(by_id(&sim)[&1], moving(1, a, b, Direction::Forward, 0));
    }
}

#[test]
fn blocked_automatic_reversal_commits_direction_only_on_successful_retry() {
    let (network, [a, b, c]) = line(["A", "B", "C"]);
    let mut sim = Simulation::new(
        network,
        vec![
            Train::new(TrainId(1), 100, c, Direction::Forward, 3),
            Train::new_manual(TrainId(2), 100, b, Direction::Backward, 3),
        ],
        DwellPolicy::new(),
    );
    for _ in 0..3 {
        sim.step();
    }
    assert_eq!(by_id(&sim)[&1], ready(1, c, Direction::Forward));
    sim.step();
    assert_eq!(by_id(&sim)[&1], ready(1, c, Direction::Forward));
    sim.apply_command(TrainCommand::Accelerate {
        train_id: TrainId(2),
    })
    .unwrap();
    assert_eq!(by_id(&sim)[&2], moving(2, b, a, Direction::Backward, 0));
    sim.step();
    assert_eq!(by_id(&sim)[&1], moving(1, c, b, Direction::Backward, 0));
}

#[test]
fn terminal_contention_uses_numeric_id_in_both_storage_orders() {
    let mut outcomes = Vec::new();
    for reversed in [false, true] {
        let (network, [a, b]) = line(["A", "B"]);
        let mut trains = vec![
            Train::new(TrainId(2), 100, b, Direction::Forward, 3),
            Train::new(TrainId(9), 100, b, Direction::Backward, 3),
        ];
        if reversed {
            trains.reverse();
        }
        let order = trains.iter().map(|train| train.id.0).collect::<Vec<_>>();
        let mut sim = Simulation::new(network, trains, DwellPolicy::new());
        for _ in 0..3 {
            sim.step();
            assert_order(&sim, &order);
        }
        assert_eq!(sim.elapsed_seconds, 3);
        let outcome = by_id(&sim);
        assert_eq!(outcome[&2], moving(2, b, a, Direction::Backward, 0));
        assert_eq!(outcome[&9], ready(9, b, Direction::Backward));
        outcomes.push(outcome);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn lower_id_proposal_cannot_displace_existing_destination_owner() {
    let (network, [a, b]) = line(["A", "B"]);
    let mut sim = Simulation::new(
        network,
        vec![
            Train::new(TrainId(1), 100, a, Direction::Forward, 3),
            Train::new_manual(TrainId(100), 100, b, Direction::Forward, 3),
        ],
        DwellPolicy::new(),
    );
    for _ in 0..3 {
        sim.step();
    }
    for time in 3..=5 {
        assert_eq!(sim.elapsed_seconds, time);
        assert_eq!(by_id(&sim)[&1], ready(1, a, Direction::Forward));
        assert_eq!(by_id(&sim)[&100], ready(100, b, Direction::Forward));
        if time < 5 {
            sim.step();
        }
    }
}

#[test]
fn blocked_automatic_current_direction_never_falls_back_to_available_reverse_track() {
    let (network, [a, b, c, d]) = line(["A", "B", "C", "D"]);
    let mut sim = Simulation::new(
        network,
        vec![
            Train::new(TrainId(1), 100, b, Direction::Forward, 3),
            Train::new_manual(TrainId(2), 100, c, Direction::Forward, 3),
        ],
        DwellPolicy::new(),
    );
    // B -> A exists and A/Backward is empty throughout the blocked retries.
    assert!(sim.network.track(b, a).is_some());
    for time in 1..=5 {
        sim.step();
        assert_eq!(sim.elapsed_seconds, time);
        if time >= 3 {
            assert_eq!(by_id(&sim)[&1], ready(1, b, Direction::Forward));
            assert_eq!(by_id(&sim)[&2], ready(2, c, Direction::Forward));
        }
    }

    sim.apply_command(TrainCommand::Accelerate {
        train_id: TrainId(2),
    })
    .unwrap();
    assert_eq!(by_id(&sim)[&2], moving(2, c, d, Direction::Forward, 0));
    sim.step();
    assert_eq!(sim.elapsed_seconds, 6);
    assert_eq!(by_id(&sim)[&1], moving(1, b, c, Direction::Forward, 0));
}
