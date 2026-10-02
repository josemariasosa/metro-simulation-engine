//! SPEC-003 checkpoint 5: all assertions concern completed transaction boundaries.
use crate::command::{CommandError, TrainCommand};
use crate::domain::resource::ResourceView;
use crate::dwell::DwellPolicy;
use crate::network::Network;
use crate::simulation::Simulation;
use crate::station::StationId;
use crate::test_utils::utils::{assert_claims, assert_physical_invariants};
use crate::train::{AtStationState, Direction, Train, TrainId, TrainState};

use Direction::{Backward as B, Forward as F};

fn line(n: usize, trains: Vec<Train>) -> Simulation {
    let mut network = Network::new();
    for i in 0..n {
        network.add_station(&i.to_string());
    }
    for i in 1..n {
        network.connect_bidirectional(StationId(i - 1), StationId(i), 2);
    }
    let sim = Simulation::new(network, trains, DwellPolicy::new());
    assert_physical_invariants(&sim);
    sim
}

fn automatic(id: usize, station: usize, direction: Direction) -> Train {
    Train::new(
        TrainId(id),
        100,
        StationId(station),
        direction,
        DwellPolicy::default_dwell_seconds(),
    )
}

fn manual(id: usize, station: usize, direction: Direction) -> Train {
    Train::new_manual(
        TrainId(id),
        100,
        StationId(station),
        direction,
        DwellPolicy::default_dwell_seconds(),
    )
}

fn step(sim: &mut Simulation) {
    let time = sim.elapsed_seconds;
    sim.step();
    assert_eq!(sim.elapsed_seconds, time + 1);
    assert_physical_invariants(sim);
}

fn advance(sim: &mut Simulation, seconds: usize) {
    for _ in 0..seconds {
        step(sim);
    }
}

// Compare every committed train field, including fields absent from snapshots.
fn assert_world(sim: &Simulation, time: u64, trains: &[Train]) {
    assert_eq!(sim.elapsed_seconds, time);
    assert_eq!(sim.trains().len(), trains.len());
    for (actual, expected) in sim.trains().iter().zip(trains) {
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.capacity, expected.capacity);
        assert_eq!(actual.control, expected.control);
        assert_eq!(actual.direction, expected.direction);
        assert_eq!(actual.velocity, expected.velocity);
        assert_eq!(actual.state, expected.state);
    }
}

fn command(sim: &mut Simulation, id: usize, expected: Result<(), CommandError>) {
    let time = sim.elapsed_seconds;
    let before = sim.trains().to_vec();
    let already_moving = before
        .iter()
        .any(|t| t.id == TrainId(id) && matches!(t.state, TrainState::Moving { .. }));
    assert_eq!(
        sim.apply_command(TrainCommand::Accelerate {
            train_id: TrainId(id)
        }),
        expected
    );
    assert_eq!(sim.elapsed_seconds, time);
    assert_physical_invariants(sim);
    if expected.is_err() || already_moving {
        assert_world(sim, time, &before);
    }
}

fn train(sim: &Simulation, id: usize) -> &Train {
    sim.trains().iter().find(|t| t.id == TrainId(id)).unwrap()
}

fn moving(sim: &Simulation, id: usize, from: usize, to: usize, elapsed: u64) {
    assert_eq!(
        train(sim, id).state,
        TrainState::Moving {
            from: StationId(from),
            to: StationId(to),
            elapsed_seconds: elapsed,
        }
    );
    assert_eq!(train(sim, id).direction, if to > from { F } else { B });
    assert_eq!(train(sim, id).velocity, 1);
}

fn ready(sim: &Simulation, id: usize, station: usize, direction: Direction) {
    assert_eq!(
        train(sim, id).state,
        TrainState::AtStation {
            station: StationId(station),
            state: AtStationState::Ready,
        }
    );
    assert_eq!(train(sim, id).direction, direction);
    assert_eq!(train(sim, id).velocity, 0);
}

fn arrived(sim: &Simulation, id: usize, station: usize, direction: Direction) {
    let t = train(sim, id);
    assert_eq!(
        t.state,
        TrainState::AtStation {
            station: StationId(station),
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: DwellPolicy::new().dwell_seconds(StationId(station), t),
            },
        }
    );
    assert_eq!(t.direction, direction);
    assert_eq!(t.velocity, 0);
}

#[test]
fn reservation_converts_atomically_to_occupancy_and_arrival_keeps_direction() {
    let mut sim = line(2, vec![manual(1, 0, F)]);
    let (a, b, id) = (StationId(0), StationId(1), TrainId(1));
    assert_claims(sim.trains(), &[(a, F, id)], &[], &[]);
    command(&mut sim, 1, Ok(()));
    for elapsed in 0..2 {
        moving(&sim, 1, 0, 1, elapsed);
        assert_claims(sim.trains(), &[], &[(b, F, id)], &[(a, b, id)]);
        step(&mut sim);
    }
    arrived(&sim, 1, 1, F);
    assert_claims(sim.trains(), &[(b, F, id)], &[], &[]);
    // Arrival is still Forward at the terminal; only a later accepted command reverses.
    let old = sim.snapshot();
    let retained = old.clone();
    command(&mut sim, 1, Ok(()));
    moving(&sim, 1, 1, 0, 0);
    assert_eq!(old, retained);
    advance(&mut sim, 2);
    arrived(&sim, 1, 0, B);
}

#[test]
fn reservation_is_exclusive_through_traversal_without_readmission() {
    let mut sim = line(2, vec![manual(1, 1, F), manual(2, 1, B)]);
    let (a, b) = (StationId(0), StationId(1));
    command(&mut sim, 1, Ok(()));
    // Reversal releases B/Forward without acquiring occupied B/Backward.
    for elapsed in 0..2 {
        moving(&sim, 1, 1, 0, elapsed);
        assert_claims(
            sim.trains(),
            &[(b, B, TrainId(2))],
            &[(a, B, TrainId(1))],
            &[(b, a, TrainId(1))],
        );
        command(&mut sim, 2, Err(CommandError::Blocked));
        // Its own track and reservation are unavailable for a NEW admission,
        // but neither a movement tick nor Accelerate may re-admit this train.
        command(&mut sim, 1, Ok(()));
        step(&mut sim);
    }
    arrived(&sim, 1, 0, B);
    assert_claims(
        sim.trains(),
        &[(a, B, TrainId(1)), (b, B, TrainId(2))],
        &[],
        &[],
    );
    assert!(ResourceView::derive(sim.trains()).track_available(b, a));
    // Now only destination occupancy can reject the command: the track is free.
    command(&mut sim, 2, Err(CommandError::Blocked));
}

#[test]
fn opposite_direction_arrivals_share_a_logical_station() {
    let mut sim = line(3, vec![automatic(1, 0, F), automatic(2, 2, B)]);
    advance(&mut sim, 3);
    for elapsed in 0..2 {
        moving(&sim, 1, 0, 1, elapsed);
        moving(&sim, 2, 2, 1, elapsed);
        assert_claims(
            sim.trains(),
            &[],
            &[(StationId(1), F, TrainId(1)), (StationId(1), B, TrainId(2))],
            &[
                (StationId(0), StationId(1), TrainId(1)),
                (StationId(2), StationId(1), TrainId(2)),
            ],
        );
        step(&mut sim);
    }
    arrived(&sim, 1, 1, F);
    arrived(&sim, 2, 1, B);
}

#[test]
fn reverse_directed_tracks_are_independent_through_arrival() {
    let mut sim = line(2, vec![automatic(1, 0, F), automatic(2, 1, B)]);
    advance(&mut sim, 3);
    for elapsed in 0..2 {
        moving(&sim, 1, 0, 1, elapsed);
        moving(&sim, 2, 1, 0, elapsed);
        step(&mut sim);
    }
    arrived(&sim, 1, 1, F);
    arrived(&sim, 2, 0, B);
    advance(&mut sim, 3);
    // Their reverse destinations are now occupied by each other. Independent
    // tracks permit the first traversal, not a swap of occupied station slots.
    for _ in 0..3 {
        ready(&sim, 1, 1, F);
        ready(&sim, 2, 0, B);
        step(&mut sim);
    }
}

#[test]
fn serial_manual_contention_is_won_by_call_order_not_id() {
    for (first, second) in [(9, 2), (2, 9)] {
        let mut sim = line(2, vec![manual(9, 1, F), manual(2, 1, B)]);
        command(&mut sim, first, Ok(()));
        command(&mut sim, second, Err(CommandError::Blocked));
        moving(&sim, first, 1, 0, 0);
        advance(&mut sim, 2);
        arrived(&sim, first, 0, B);
        command(&mut sim, second, Err(CommandError::Blocked));
    }
}

#[test]
fn manual_command_sees_resources_released_by_completed_automatic_step() {
    let mut sim = line(
        3,
        vec![automatic(1, 0, F), automatic(2, 1, F), manual(9, 0, B)],
    );
    advance(&mut sim, 3);
    // Frozen World N prevents automatic T1 from using B in the departure step.
    ready(&sim, 1, 0, F);
    moving(&sim, 2, 1, 2, 0);
    ready(&sim, 9, 0, B);
    command(&mut sim, 9, Ok(()));
    assert_eq!(sim.elapsed_seconds, 3);
    moving(&sim, 9, 0, 1, 0);
    ready(&sim, 1, 0, F);
    advance(&mut sim, 2);
    arrived(&sim, 9, 1, F);
    arrived(&sim, 2, 2, F);
    ready(&sim, 1, 0, F);
}

#[test]
fn occupied_slot_retry_and_manual_no_buffering_preserve_invariants() {
    for is_manual in [false, true] {
        let follower = if is_manual {
            manual(1, 0, F)
        } else {
            automatic(1, 0, F)
        };
        let mut sim = line(3, vec![follower, manual(2, 1, F)]);
        if is_manual {
            command(&mut sim, 1, Err(CommandError::Blocked));
        }
        advance(&mut sim, 3);
        ready(&sim, 1, 0, F);
        if is_manual {
            command(&mut sim, 1, Err(CommandError::Blocked));
        }
        step(&mut sim);
        ready(&sim, 1, 0, F);
        command(&mut sim, 2, Ok(()));
        step(&mut sim);
        if is_manual {
            ready(&sim, 1, 0, F);
            command(&mut sim, 1, Ok(()));
        }
        moving(&sim, 1, 0, 1, 0);
        advance(&mut sim, 2);
        arrived(&sim, 1, 1, F);
    }
}

#[test]
fn blocked_reversal_retains_direction_until_accepted_departure() {
    for is_manual in [false, true] {
        let follower = if is_manual {
            manual(1, 2, F)
        } else {
            automatic(1, 2, F)
        };
        let mut sim = line(3, vec![follower, manual(2, 1, B)]);
        if is_manual {
            command(&mut sim, 1, Err(CommandError::Blocked));
        }
        advance(&mut sim, 3);
        ready(&sim, 1, 2, F);
        if is_manual {
            command(&mut sim, 1, Err(CommandError::Blocked));
        }
        step(&mut sim);
        ready(&sim, 1, 2, F);
        command(&mut sim, 2, Ok(()));
        step(&mut sim);
        if is_manual {
            ready(&sim, 1, 2, F);
            command(&mut sim, 1, Ok(()));
        }
        moving(&sim, 1, 2, 1, 0);
        advance(&mut sim, 2);
        arrived(&sim, 1, 1, B);
    }
}

fn assert_order(sim: &Simulation, order: &[TrainId]) {
    assert_eq!(sim.trains().iter().map(|t| t.id).collect::<Vec<_>>(), order);
    assert_eq!(
        sim.snapshot()
            .trains
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        order
    );
}

#[test]
fn conga_and_terminal_contention_are_independent_of_observation_and_storage_order() {
    for conga in [true, false] {
        let mut ordered_trace = Vec::new();
        for reversed in [false, true] {
            let mut initial = if conga {
                vec![automatic(1, 0, F), automatic(2, 1, F), automatic(3, 2, F)]
            } else {
                vec![automatic(2, 1, F), automatic(9, 1, B)]
            };
            if reversed {
                initial.reverse();
            }
            let order = initial.iter().map(|t| t.id).collect::<Vec<_>>();
            let n = if conga { 4 } else { 2 };
            let mut frequent = line(n, initial.clone());
            let mut sparse = line(n, initial);
            let old = frequent.snapshot();
            let retained = old.clone();
            for time in 0..=16 {
                assert_world(&sparse, frequent.elapsed_seconds, frequent.trains());
                assert_order(&frequent, &order);
                assert_eq!(frequent.snapshot(), frequent.snapshot());
                let mut canonical = frequent.snapshot();
                canonical.trains.sort_by_key(|t| t.id.0);
                if reversed {
                    assert_eq!(canonical, ordered_trace[time]);
                } else {
                    ordered_trace.push(canonical);
                }
                match (conga, time) {
                    (true, 3) => {
                        ready(&frequent, 1, 0, F);
                        ready(&frequent, 2, 1, F);
                        moving(&frequent, 3, 2, 3, 0);
                    }
                    (true, 4) => {
                        ready(&frequent, 1, 0, F);
                        moving(&frequent, 2, 1, 2, 0);
                        moving(&frequent, 3, 2, 3, 1);
                    }
                    (true, 5) => {
                        moving(&frequent, 1, 0, 1, 0);
                        moving(&frequent, 2, 1, 2, 1);
                        arrived(&frequent, 3, 3, F);
                    }
                    (false, 3) => {
                        moving(&frequent, 2, 1, 0, 0);
                        ready(&frequent, 9, 1, B);
                    }
                    (false, 5) => {
                        arrived(&frequent, 2, 0, B);
                        ready(&frequent, 9, 1, B);
                    }
                    (false, 8) => {
                        moving(&frequent, 2, 0, 1, 0);
                        ready(&frequent, 9, 1, B);
                    }
                    (false, 9) => moving(&frequent, 9, 1, 0, 0),
                    _ => {}
                }
                if time < 16 {
                    step(&mut frequent);
                    step(&mut sparse);
                }
            }
            // This is the sparse run's first snapshot, after all intermediate boundaries.
            assert_eq!(frequent.snapshot(), sparse.snapshot());
            assert_order(&sparse, &order);
            assert_eq!(old, retained);
        }
    }
}
