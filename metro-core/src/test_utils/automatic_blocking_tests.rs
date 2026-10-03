use std::collections::BTreeMap;

use crate::dwell::DwellPolicy;
use crate::network::Network;
use crate::simulation::{Simulation, TrainId};
use crate::snapshot::{TrainSnapshot, TrainSnapshotState};
use crate::station::StationId;
use crate::train::{AtStationState, Direction, Train, TrainState};

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
    assert_eq!(sim.trains().iter().map(|e| e.id.0).collect::<Vec<_>>(), ids);
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
        let trains = vec![
            Train::new(100, a, Direction::Forward, 3),
            Train::new(100, b, Direction::Forward, 3),
            Train::new(100, c, Direction::Forward, 3),
        ];
        let mut sim = Simulation::new(network, trains, DwellPolicy::new());
        if reversed {
            sim.reverse_train_order_for_test();
        }
        let order = sim.trains().iter().map(|e| e.id.0).collect::<Vec<_>>();
        let mut trace = Vec::new();
        let expected = [
            vec![
                ready(0, a, Direction::Forward),
                ready(1, b, Direction::Forward),
                moving(2, c, d, Direction::Forward, 0),
            ],
            vec![
                ready(0, a, Direction::Forward),
                moving(1, b, c, Direction::Forward, 0),
                moving(2, c, d, Direction::Forward, 1),
            ],
            vec![
                moving(0, a, b, Direction::Forward, 0),
                moving(1, b, c, Direction::Forward, 1),
                TrainSnapshot {
                    id: TrainId(2),
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
        let t3 = sim.trains().iter().find(|e| e.id == TrainId(2)).unwrap();
        assert_eq!(
            t3.train.state(),
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
        let trains = vec![
            Train::new(100, b, Direction::Forward, 3),
            Train::new(100, b, Direction::Backward, 3),
        ];
        let mut sim = Simulation::new(network, trains, DwellPolicy::new());
        if reversed {
            sim.reverse_train_order_for_test();
        }
        let order = sim.trains().iter().map(|e| e.id.0).collect::<Vec<_>>();
        for _ in 0..3 {
            sim.step();
            assert_order(&sim, &order);
        }
        assert_eq!(sim.elapsed_seconds, 3);
        let outcome = by_id(&sim);
        assert_eq!(outcome[&0], moving(0, b, a, Direction::Backward, 0));
        assert_eq!(outcome[&1], ready(1, b, Direction::Backward));
        outcomes.push(outcome);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}
