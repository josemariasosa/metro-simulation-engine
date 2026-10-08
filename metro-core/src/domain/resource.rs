use crate::domain::station::StationId;
use crate::domain::train::{Direction, Train, TrainState};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceConflict {
    StationSlot(StationSlot),
    Track(StationId, StationId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StationSlot {
    pub(crate) station: StationId,
    pub(crate) direction: Direction,
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ResourceView<Owner> {
    pub(crate) station_occupants: HashMap<StationSlot, Owner>,
    pub(crate) station_reservations: HashMap<StationSlot, Owner>,
    pub(crate) track_occupants: HashMap<(StationId, StationId), Owner>,
}

impl<Owner> Default for ResourceView<Owner> {
    fn default() -> Self {
        Self {
            station_occupants: HashMap::new(),
            station_reservations: HashMap::new(),
            track_occupants: HashMap::new(),
        }
    }
}

impl<Owner: Copy> ResourceView<Owner> {
    pub(crate) fn track_available(&self, from: StationId, to: StationId) -> bool {
        !self.track_occupants.contains_key(&(from, to))
    }

    pub(crate) fn station_slot_available(&self, station: StationId, direction: Direction) -> bool {
        let slot = StationSlot { station, direction };
        !self.station_occupants.contains_key(&slot)
            && !self.station_reservations.contains_key(&slot)
    }

    fn try_reserve_station(
        &mut self,
        slot: StationSlot,
        owner: Owner,
    ) -> Result<(), ResourceConflict> {
        if self.station_occupants.contains_key(&slot)
            || self.station_reservations.contains_key(&slot)
        {
            return Err(ResourceConflict::StationSlot(slot));
        }

        self.station_reservations.insert(slot, owner);
        Ok(())
    }

    /// Derives physical claims from domain trains paired with opaque owner tokens.
    /// The caller supplies identity; resource rules never interpret or order owners.
    pub(crate) fn derive<'a>(trains: impl IntoIterator<Item = (Owner, &'a Train)>) -> Self {
        Self::try_derive(trains).expect("invalid resource ownership")
    }

    pub(crate) fn try_derive<'a>(
        trains: impl IntoIterator<Item = (Owner, &'a Train)>,
    ) -> Result<Self, ResourceConflict> {
        let mut view = Self::default();
        for (owner, train) in trains {
            match train.state() {
                TrainState::AtStation { station, .. } => {
                    let slot = StationSlot {
                        station,
                        direction: train.direction(),
                    };

                    if view.station_reservations.contains_key(&slot)
                        || view.station_occupants.contains_key(&slot)
                    {
                        return Err(ResourceConflict::StationSlot(slot));
                    }

                    view.station_occupants.insert(slot, owner);
                }
                TrainState::Moving { from, to, .. } => {
                    if view.track_occupants.contains_key(&(from, to)) {
                        return Err(ResourceConflict::Track(from, to));
                    }

                    view.track_occupants.insert((from, to), owner);

                    view.try_reserve_station(
                        StationSlot {
                            station: to,
                            direction: train.direction(),
                        },
                        owner,
                    )?;
                }
            }
        }
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::dwell::DwellPolicy;
    use crate::domain::network::Network;
    use crate::domain::train::{AtStationState, Train};
    fn owned_moving_train(
        id: usize,
        from: StationId,
        to: StationId,
        direction: Direction,
    ) -> (usize, Train) {
        let mut train = Train::new(100, from, direction, DwellPolicy::default_dwell_seconds());
        train.set_moving_for_test(from, to, 0);
        (id, train)
    }

    fn resources(trains: &[(usize, Train)]) -> ResourceView<usize> {
        ResourceView::derive(trains.iter().map(|(owner, train)| (*owner, train)))
    }

    #[test]
    fn availability_checks_track_occupancy_and_both_slot_claims() {
        let a = StationId(0);
        let b = StationId(1);
        let empty = ResourceView::<usize>::default();
        assert!(empty.track_available(a, b));
        assert!(empty.station_slot_available(b, Direction::Forward));

        let trains = &[(
            1,
            Train::new(
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        )];
        let occupied = resources(trains);
        assert!(occupied.track_available(a, b));
        assert!(!occupied.station_slot_available(b, Direction::Forward));
        assert!(occupied.station_slot_available(b, Direction::Backward));

        let reserved = resources(&[owned_moving_train(1, a, b, Direction::Forward)]);
        assert!(!reserved.track_available(a, b));
        assert!(reserved.track_available(b, a));
        assert!(!reserved.station_slot_available(b, Direction::Forward));
        assert!(reserved.station_slot_available(b, Direction::Backward));
        assert!(reserved.station_slot_available(a, Direction::Forward));
    }

    #[test]
    fn resource_view_preserves_opposite_slot_owners() {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");
        network.connect_bidirectional(a, b, 2);
        network.connect_bidirectional(b, c, 2);

        let trains = vec![
            (
                1,
                Train::new(
                    100,
                    b,
                    Direction::Forward,
                    DwellPolicy::default_dwell_seconds(),
                ),
            ),
            (
                2,
                Train::new(
                    100,
                    b,
                    Direction::Backward,
                    DwellPolicy::default_dwell_seconds(),
                ),
            ),
        ];
        let forward_slot = StationSlot {
            station: b,
            direction: Direction::Forward,
        };
        let backward_slot = StationSlot {
            station: b,
            direction: Direction::Backward,
        };

        let view = resources(&trains);

        assert_ne!(forward_slot, backward_slot);
        assert_eq!(view.station_occupants.len(), 2);
        assert_eq!(view.station_occupants.get(&forward_slot), Some(&1));
        assert_eq!(view.station_occupants.get(&backward_slot), Some(&2));
        assert!(view.station_reservations.is_empty());
        assert!(view.track_occupants.is_empty());
    }

    #[test]
    #[should_panic(expected = "station slot already occupied")]
    fn resource_view_rejects_duplicate_station_occupants() {
        let train = Train::new(
            100,
            StationId(0),
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let first = (1, train.clone());
        let other = (2, train);
        resources(&[first, other]);
    }

    #[test]
    #[should_panic(expected = "directed track already occupied")]
    fn resource_view_rejects_duplicate_track_occupants() {
        resources(&[
            owned_moving_train(1, StationId(0), StationId(1), Direction::Forward),
            owned_moving_train(2, StationId(0), StationId(1), Direction::Forward),
        ]);
    }

    #[test]
    #[should_panic(expected = "destination slot already reserved")]
    fn resource_view_rejects_duplicate_reservations() {
        let mut view = ResourceView::default();
        let slot = StationSlot {
            station: StationId(1),
            direction: Direction::Forward,
        };

        assert_eq!(view.try_reserve_station(slot, 1), Ok(()));
        assert_eq!(
            view.try_reserve_station(slot, 2),
            Err(ResourceConflict::StationSlot(slot)),
        );
    }

    #[test]
    #[should_panic(expected = "station slot occupied and reserved")]
    fn resource_view_rejects_reserving_occupied_slot() {
        let stopped = Train::new(
            100,
            StationId(1),
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let moving = owned_moving_train(2, StationId(0), StationId(1), Direction::Forward);
        resources(&[(1, stopped), moving]);
    }

    #[test]
    #[should_panic(expected = "station slot occupied and reserved")]
    fn resource_view_rejects_occupying_reserved_slot() {
        let stopped = (
            1,
            Train::new(
                100,
                StationId(1),
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        );
        let moving = owned_moving_train(2, StationId(0), StationId(1), Direction::Forward);
        resources(&[moving, stopped]);
    }

    #[test]
    fn resource_view_allows_opposite_direction_station_slots() {
        for state in [
            AtStationState::Ready,
            AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3,
            },
        ] {
            let mut forward = (
                1,
                Train::new(
                    100,
                    StationId(1),
                    Direction::Forward,
                    DwellPolicy::default_dwell_seconds(),
                ),
            );
            match state {
                AtStationState::Ready => {
                    forward.1.set_ready_for_test(StationId(1));
                }
                AtStationState::Dwelling {
                    elapsed_seconds: _,
                    dwell_seconds,
                } => {
                    forward.1.set_dwell_for_test(StationId(1), dwell_seconds);
                }
            }

            let mut backward = (2, forward.1.clone());
            backward.1.set_direction_for_test(Direction::Backward);
            let view = resources(&[forward, backward]);
            for (direction, id) in [(Direction::Forward, 1), (Direction::Backward, 2)] {
                assert_eq!(
                    view.station_occupants.get(&StationSlot {
                        station: StationId(1),
                        direction
                    }),
                    Some(&id)
                );
            }
            assert_eq!(view.station_occupants.len(), 2);
            assert!(view.station_reservations.is_empty());
            assert!(view.track_occupants.is_empty());
        }
    }

    #[test]
    fn moving_train_occupies_track_and_reserves_destination_slot() {
        let view = resources(&[owned_moving_train(
            1,
            StationId(0),
            StationId(1),
            Direction::Forward,
        )]);
        assert_eq!(
            view.track_occupants.get(&(StationId(0), StationId(1))),
            Some(&1)
        );
        assert_eq!(
            view.station_reservations.get(&StationSlot {
                station: StationId(1),
                direction: Direction::Forward
            }),
            Some(&1)
        );
        assert_eq!(view.track_occupants.len(), 1);
        assert_eq!(view.station_reservations.len(), 1);
        assert!(view.station_occupants.is_empty());
    }
}
