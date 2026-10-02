use crate::station::StationId;
use crate::train::{Direction, Train, TrainId, TrainState};

/// Checks committed state without taking an observation, including in sparse-observation runs.
pub(crate) fn assert_physical_invariants(sim: &crate::simulation::Simulation) {
    let mut ids = std::collections::HashSet::new();
    let mut occupants = Vec::new();
    let mut reservations = Vec::new();
    let mut tracks = Vec::new();
    for train in sim.trains() {
        assert!(ids.insert(train.id), "duplicate TrainId");
        match train.state {
            TrainState::AtStation { station, .. } => {
                assert_eq!(train.velocity, 0);
                occupants.push((station, train.direction, train.id));
            }
            TrainState::Moving {
                from,
                to,
                elapsed_seconds,
            } => {
                assert_eq!(train.velocity, 1);
                match train.direction {
                    Direction::Forward => assert_eq!(to.0, from.0 + 1),
                    Direction::Backward => assert_eq!(from.0, to.0 + 1),
                }
                let track = sim
                    .network
                    .track(from, to)
                    .expect("existing movement track");
                assert!(track.travel_seconds > 0);
                assert!(elapsed_seconds < track.travel_seconds);
                tracks.push((from, to, train.id));
                reservations.push((to, train.direction, train.id));
            }
        }
    }
    // Exact map equality also excludes extra ownership: stopped trains have only
    // occupancy; moving trains have only their track and destination reservation.
    crate::resource::assert_claims(sim.trains(), &occupants, &reservations, &tracks);
}

pub fn moving_train(id: usize, from: StationId, to: StationId, direction: Direction) -> Train {
    let mut train = Train::new(TrainId(id), 100, from, direction);
    train.state = TrainState::Moving {
        from,
        to,
        elapsed_seconds: 0,
    };
    train.velocity = 1;
    train
}
