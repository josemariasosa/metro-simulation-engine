use crate::domain::resource::{ResourceView, StationSlot};
use crate::domain::train::{Direction, Train, TrainState};
use crate::dwell::DwellPolicy;
use crate::simulation::{TrainEntity, TrainId};
use crate::station::StationId;
use std::collections::HashMap;

/// Checks committed state without taking an observation, including in sparse-observation runs.
pub(crate) fn assert_physical_invariants(sim: &crate::simulation::Simulation) {
    let mut occupants = Vec::new();
    let mut reservations = Vec::new();
    let mut tracks = Vec::new();
    for TrainEntity {
        id: train_id,
        train,
    } in sim.trains()
    {
        match train.state() {
            TrainState::AtStation { station, .. } => {
                assert_eq!(train.velocity(), 0);
                occupants.push((station, train.direction(), *train_id));
            }
            TrainState::Moving {
                from,
                to,
                elapsed_seconds,
            } => {
                assert_eq!(train.velocity(), 1);
                match train.direction() {
                    Direction::Forward => assert_eq!(to.0, from.0 + 1),
                    Direction::Backward => assert_eq!(from.0, to.0 + 1),
                }
                let track = sim
                    .network
                    .track(from, to)
                    .expect("existing movement track");
                assert!(track.travel_seconds > 0);
                assert!(elapsed_seconds < track.travel_seconds);
                tracks.push((from, to, *train_id));
                reservations.push((to, train.direction(), *train_id));
            }
        }
    }
    // Exact map equality also excludes extra ownership: stopped trains have only
    // occupancy; moving trains have only their track and destination reservation.
    assert_claims(sim.trains(), &occupants, &reservations, &tracks);
}

pub(crate) fn assert_claims(
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
    let actual = ResourceView::derive(trains);
    assert_eq!(actual.station_occupants, expected_occupants);
    assert_eq!(actual.station_reservations, expected_reservations);
    assert_eq!(actual.track_occupants, expected_tracks);
}

pub(crate) fn moving_train(from: StationId, to: StationId, direction: Direction) -> Train {
    let mut train = Train::new(100, from, direction, DwellPolicy::default_dwell_seconds());
    train.set_moving_for_test(from, to, 0);
    train
}

/// Explicit ownership belongs only in tests of derived resource claims.
pub(crate) fn moving_train_entity(
    id: usize,
    from: StationId,
    to: StationId,
    direction: Direction,
) -> TrainEntity {
    TrainEntity {
        id: TrainId(id),
        train: moving_train(from, to, direction),
    }
}
