use std::collections::HashMap;

use metro_core::DwellPolicy;
use metro_core::Network;
use metro_core::StationId;
use metro_core::command::TrainCommand;
use metro_core::simulation::Simulation;
use metro_core::snapshot::{TrainSnapshot, TrainSnapshotState};
use metro_core::{Direction, Train, TrainId};

fn line<const N: usize>(names: [&str; N]) -> (Network, [StationId; N]) {
    let mut network = Network::new();
    let stations = names.map(|name| network.add_station(name));
    for pair in stations.windows(2) {
        network.connect_bidirectional(pair[0], pair[1], 2);
    }
    (network, stations)
}

fn ready(id: TrainId, station: StationId, direction: Direction) -> TrainSnapshot {
    TrainSnapshot {
        id,
        direction,
        velocity: 0,
        state: TrainSnapshotState::Ready { station },
    }
}

fn moving(
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

#[test]
fn blocked_automatic_retries_without_restarting_dwell() {
    for extra_blocked_steps in [0, 3] {
        let (network, [a, b, c]) = line(["A", "B", "C"]);
        let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
        let automatic_id = sim
            .add_train(Train::new(100, a, Direction::Forward, 3))
            .unwrap();
        let manual_id = sim
            .add_train(Train::new_manual(100, b, Direction::Forward, 3))
            .unwrap();
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(
            by_id(&sim)[&automatic_id],
            ready(automatic_id, a, Direction::Forward)
        );
        for _ in 0..extra_blocked_steps {
            sim.step();
            assert_eq!(
                by_id(&sim)[&automatic_id],
                ready(automatic_id, a, Direction::Forward)
            );
        }
        sim.apply_command(TrainCommand::Accelerate {
            train_id: manual_id,
        })
        .unwrap();
        assert_eq!(
            by_id(&sim)[&manual_id],
            moving(manual_id, b, c, Direction::Forward, 0)
        );
        sim.step();
        assert_eq!(sim.elapsed_seconds, 4 + extra_blocked_steps);
        assert_eq!(
            by_id(&sim)[&automatic_id],
            moving(automatic_id, a, b, Direction::Forward, 0)
        );
    }
}

#[test]
fn blocked_automatic_reversal_commits_direction_only_on_successful_retry() {
    let (network, [a, b, c]) = line(["A", "B", "C"]);
    let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
    let automatic_id = sim
        .add_train(Train::new(100, c, Direction::Forward, 3))
        .unwrap();
    let manual_id = sim
        .add_train(Train::new_manual(100, b, Direction::Backward, 3))
        .unwrap();
    for _ in 0..3 {
        sim.step();
    }
    assert_eq!(
        by_id(&sim)[&automatic_id],
        ready(automatic_id, c, Direction::Forward)
    );
    sim.step();
    assert_eq!(
        by_id(&sim)[&automatic_id],
        ready(automatic_id, c, Direction::Forward)
    );
    sim.apply_command(TrainCommand::Accelerate {
        train_id: manual_id,
    })
    .unwrap();
    assert_eq!(
        by_id(&sim)[&manual_id],
        moving(manual_id, b, a, Direction::Backward, 0)
    );
    sim.step();
    assert_eq!(
        by_id(&sim)[&automatic_id],
        moving(automatic_id, c, b, Direction::Backward, 0)
    );
}

#[test]
fn lower_id_proposal_cannot_displace_existing_destination_owner() {
    let (network, [a, b]) = line(["A", "B"]);
    let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
    let automatic_id = sim
        .add_train(Train::new(100, a, Direction::Forward, 3))
        .unwrap();
    let manual_id = sim
        .add_train(Train::new_manual(100, b, Direction::Forward, 3))
        .unwrap();
    assert!(automatic_id.0 < manual_id.0);
    for _ in 0..3 {
        sim.step();
    }
    for time in 3..=5 {
        assert_eq!(sim.elapsed_seconds, time);
        assert_eq!(
            by_id(&sim)[&automatic_id],
            ready(automatic_id, a, Direction::Forward)
        );
        assert_eq!(
            by_id(&sim)[&manual_id],
            ready(manual_id, b, Direction::Forward)
        );
        if time < 5 {
            sim.step();
        }
    }
}

#[test]
fn blocked_automatic_current_direction_never_falls_back_to_available_reverse_track() {
    let (network, [a, b, c, d]) = line(["A", "B", "C", "D"]);
    let mut sim = Simulation::new(network, vec![], DwellPolicy::new());
    let automatic_id = sim
        .add_train(Train::new(100, b, Direction::Forward, 3))
        .unwrap();
    let manual_id = sim
        .add_train(Train::new_manual(100, c, Direction::Forward, 3))
        .unwrap();
    // B -> A exists and A/Backward is empty throughout the blocked retries.
    assert!(sim.network.track(b, a).is_some());
    for time in 1..=5 {
        sim.step();
        assert_eq!(sim.elapsed_seconds, time);
        if time >= 3 {
            assert_eq!(
                by_id(&sim)[&automatic_id],
                ready(automatic_id, b, Direction::Forward)
            );
            assert_eq!(
                by_id(&sim)[&manual_id],
                ready(manual_id, c, Direction::Forward)
            );
        }
    }

    sim.apply_command(TrainCommand::Accelerate {
        train_id: manual_id,
    })
    .unwrap();
    assert_eq!(
        by_id(&sim)[&manual_id],
        moving(manual_id, c, d, Direction::Forward, 0)
    );
    sim.step();
    assert_eq!(sim.elapsed_seconds, 6);
    assert_eq!(
        by_id(&sim)[&automatic_id],
        moving(automatic_id, b, c, Direction::Forward, 0)
    );
}
