mod constraints;
mod construction;
mod contention;
mod departure;
mod departure_blocking;
mod dwelling;
mod movement;
mod scenarios;
mod snapshot;
mod time;
mod train_identity;

use std::collections::HashMap;

use super::*;

use crate::domain::resource::StationSlot;
use crate::domain::train::Direction::{Backward as B, Forward as F};
use crate::domain::train::{Direction, Train};

/// A fresh bidirectional line for each test. Station IDs follow the supplied order.
struct SimulationFixture<const N: usize> {
    network: Network,
    stations: [StationId; N],
    dwell_policy: DwellPolicy,
}

impl<const N: usize> SimulationFixture<N> {
    fn new(station_names: [&str; N], travel_seconds: u64) -> Self {
        let mut network = Network::new();
        let stations = station_names.map(|name| network.add_station(name));
        for pair in stations.windows(2) {
            network.connect_bidirectional(pair[0], pair[1], travel_seconds);
        }

        Self {
            network,
            stations,
            dwell_policy: DwellPolicy::new(),
        }
    }
}

fn closed(station: StationId) -> OperationalConstraint {
    OperationalConstraint::StationUnavailable { station }
}

fn constraint_simulation() -> (Simulation, [StationId; 3]) {
    let SimulationFixture {
        network,
        stations,
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let train = Train::new(100, stations[0], Direction::Forward, 3);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    simulation.add_train(train).unwrap();

    (simulation, stations)
}

fn assert_constraint_failure_atomic(
    simulation: &mut Simulation,
    expected: ConstraintError,
    register: impl FnOnce(&mut Simulation) -> Result<ConstraintId, ConstraintError>,
) {
    let snapshot = simulation.snapshot();
    let trains = committed_train_state(simulation);
    let resources = ResourceView::derive(simulation.trains());
    let next_train_id = simulation.next_train_id;
    let records = simulation.constraints.clone();
    let next_id = simulation.next_constraint_id;

    assert_eq!(register(simulation), Err(expected));

    assert_eq!(simulation.snapshot(), snapshot);
    assert_eq!(committed_train_state(simulation), trains);
    assert_eq!(ResourceView::derive(simulation.trains()), resources);
    assert_eq!(simulation.next_train_id, next_train_id);
    assert_eq!(simulation.constraints, records);
    assert_eq!(simulation.next_constraint_id, next_id);
}

fn assert_claims(
    trains: &[TrainEntity],
    occupants: &[(StationId, Direction, TrainId)],
    reservations: &[(StationId, Direction, TrainId)],
    tracks: &[(StationId, StationId, TrainId)],
) {
    let slots = |claims: &[(StationId, Direction, TrainId)]| {
        let mut result = HashMap::new();
        for &(station, direction, owner) in claims {
            assert!(
                result
                    .insert(StationSlot { station, direction }, owner)
                    .is_none()
            );
        }
        result
    };
    let expected_occupants = slots(occupants);
    let expected_reservations = slots(reservations);
    assert!(
        expected_occupants
            .keys()
            .all(|slot| !expected_reservations.contains_key(slot))
    );
    let mut expected_tracks = HashMap::new();
    for &(from, to, owner) in tracks {
        assert!(expected_tracks.insert((from, to), owner).is_none());
    }
    let actual = ResourceView::derive(trains.iter().map(|entity| (entity.id(), entity.train())));
    assert_eq!(actual.station_occupants, expected_occupants);
    assert_eq!(actual.station_reservations, expected_reservations);
    assert_eq!(actual.track_occupants, expected_tracks);
}

fn assert_physical_invariants(sim: &crate::simulation::Simulation) {
    let mut occupants = Vec::new();
    let mut reservations = Vec::new();
    let mut tracks = Vec::new();
    for e in sim.trains() {
        let train_id = e.0;
        let train = e.1;
        match train.state() {
            TrainState::AtStation { station, .. } => {
                assert_eq!(train.velocity(), 0);
                occupants.push((station, train.direction(), train_id));
            }
            TrainState::Moving {
                from,
                to,
                elapsed_seconds,
            } => {
                assert_eq!(train.velocity(), 1);
                match train.direction() {
                    F => assert_eq!(to.0, from.0 + 1),
                    B => assert_eq!(from.0, to.0 + 1),
                }
                let track = sim
                    .network
                    .track(from, to)
                    .expect("existing movement track");
                assert!(track.travel_seconds > 0);
                assert!(elapsed_seconds < track.travel_seconds);
                tracks.push((from, to, train_id));
                reservations.push((to, train.direction(), train_id));
            }
        }
    }

    // Exact map equality also excludes extra ownership: stopped trains have only
    // occupancy; moving trains have only their track and destination reservation.
    assert_claims(&sim.trains, &occupants, &reservations, &tracks);
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

pub(super) fn committed_train_state(
    sim: &Simulation,
) -> HashMap<TrainId, (usize, bool, bool, Direction, u8, TrainState)> {
    sim.trains()
        .map(|(id, train)| {
            (
                id,
                (
                    train.capacity(),
                    train.is_manual_control(),
                    train.is_automatic_control(),
                    train.direction(),
                    train.velocity(),
                    train.state(),
                ),
            )
        })
        .collect()
}

fn command(sim: &mut Simulation, train_id: TrainId, expected: Result<(), CommandError>) {
    let time = sim.elapsed_seconds;
    let before = sim.snapshot();
    let trains_before = committed_train_state(sim);
    let resources_before = ResourceView::derive(sim.trains());
    let next_train_id_before = sim.next_train_id;
    let constraints_before = sim.constraints.clone();
    let next_constraint_id_before = sim.next_constraint_id;

    let already_moving = sim
        .trains()
        .any(|(tid, train)| tid == train_id && matches!(train.state(), TrainState::Moving { .. }));

    assert_eq!(
        sim.apply_command(TrainCommand::Accelerate { train_id }),
        expected
    );

    assert_eq!(sim.elapsed_seconds, time);
    assert_physical_invariants(sim);

    if expected.is_err() || already_moving {
        assert_eq!(sim.snapshot(), before);
        assert_eq!(committed_train_state(sim), trains_before);
        assert_eq!(ResourceView::derive(sim.trains()), resources_before);
        assert_eq!(sim.next_train_id, next_train_id_before);
        assert_eq!(sim.constraints, constraints_before);
        assert_eq!(sim.next_constraint_id, next_constraint_id_before);
    }
}

fn moving(sim: &Simulation, train_id: TrainId, from: usize, to: usize, elapsed: u64) {
    let train = sim.train(train_id);
    assert_eq!(
        train.state(),
        TrainState::Moving {
            from: StationId(from),
            to: StationId(to),
            elapsed_seconds: elapsed,
        }
    );
    assert_eq!(train.direction(), if to > from { F } else { B });
    assert_eq!(train.velocity(), 1);
}

fn ready(sim: &Simulation, train_id: TrainId, station: usize, direction: Direction) {
    assert_eq!(
        sim.train(train_id).state(),
        TrainState::AtStation {
            station: StationId(station),
            state: AtStationState::Ready,
        }
    );
    assert_eq!(sim.train(train_id).direction(), direction);
    assert_eq!(sim.train(train_id).velocity(), 0);
}

fn arrived(sim: &Simulation, train_id: TrainId, station: usize, direction: Direction) {
    let t = sim.train(train_id);
    assert_eq!(
        t.state(),
        TrainState::AtStation {
            station: StationId(station),
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: DwellPolicy::new().dwell_seconds(StationId(station), t),
            },
        }
    );
    assert_eq!(t.direction(), direction);
    assert_eq!(t.velocity(), 0);
}
