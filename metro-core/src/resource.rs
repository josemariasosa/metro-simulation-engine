use crate::station::StationId;
use crate::train::{Direction, Train, TrainId, TrainState};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StationSlot {
    station: StationId,
    direction: Direction,
}

#[derive(Debug, Default)]
pub(crate) struct ResourceView {
    station_occupants: HashMap<StationSlot, TrainId>,
    station_reservations: HashMap<StationSlot, TrainId>,
    track_occupants: HashMap<(StationId, StationId), TrainId>,
}

impl ResourceView {
    fn reserve_station(&mut self, slot: StationSlot, owner: TrainId) {
        assert!(
            !self.station_occupants.contains_key(&slot),
            "station slot occupied and reserved"
        );
        assert!(
            !self.station_reservations.contains_key(&slot),
            "destination slot already reserved"
        );
        self.station_reservations.insert(slot, owner);
    }

    pub(crate) fn derive(trains: &[Train]) -> Self {
        let mut view = Self::default();
        for train in trains {
            match train.state {
                TrainState::AtStation { station, .. } => {
                    let slot = StationSlot {
                        station,
                        direction: train.direction,
                    };
                    assert!(
                        !view.station_reservations.contains_key(&slot),
                        "station slot occupied and reserved"
                    );
                    assert!(
                        !view.station_occupants.contains_key(&slot),
                        "station slot already occupied"
                    );
                    view.station_occupants.insert(slot, train.id);
                }
                TrainState::Moving { from, to, .. } => {
                    assert!(
                        !view.track_occupants.contains_key(&(from, to)),
                        "directed track already occupied"
                    );
                    view.track_occupants.insert((from, to), train.id);
                    view.reserve_station(
                        StationSlot {
                            station: to,
                            direction: train.direction,
                        },
                        train.id,
                    );
                }
            }
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::Network;
    use crate::test_utils::moving_train;
    use crate::train::AtStationState;

    #[test]
    fn resource_view_preserves_opposite_slot_owners() {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");
        network.connect_bidirectional(a, b, 2);
        network.connect_bidirectional(b, c, 2);

        let trains = vec![
            Train::new(TrainId(1), 100, b, Direction::Forward),
            Train::new(TrainId(2), 100, b, Direction::Backward),
        ];
        let forward_slot = StationSlot {
            station: b,
            direction: Direction::Forward,
        };
        let backward_slot = StationSlot {
            station: b,
            direction: Direction::Backward,
        };

        let view = ResourceView::derive(&trains);

        assert_ne!(forward_slot, backward_slot);
        assert_eq!(view.station_occupants.len(), 2);
        assert_eq!(view.station_occupants.get(&forward_slot), Some(&TrainId(1)));
        assert_eq!(
            view.station_occupants.get(&backward_slot),
            Some(&TrainId(2))
        );
        assert!(view.station_reservations.is_empty());
        assert!(view.track_occupants.is_empty());
    }

    #[test]
    #[should_panic(expected = "station slot already occupied")]
    fn resource_view_rejects_duplicate_station_occupants() {
        let train = Train::new(TrainId(1), 100, StationId(0), Direction::Forward);
        let mut other = train.clone();
        other.id = TrainId(2);
        ResourceView::derive(&[train, other]);
    }

    #[test]
    #[should_panic(expected = "directed track already occupied")]
    fn resource_view_rejects_duplicate_track_occupants() {
        ResourceView::derive(&[
            moving_train(1, StationId(0), StationId(1), Direction::Forward),
            moving_train(2, StationId(0), StationId(1), Direction::Forward),
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
        view.reserve_station(slot, TrainId(1));
        view.reserve_station(slot, TrainId(2));
    }

    #[test]
    #[should_panic(expected = "station slot occupied and reserved")]
    fn resource_view_rejects_reserving_occupied_slot() {
        let stopped = Train::new(TrainId(1), 100, StationId(1), Direction::Forward);
        let moving = moving_train(2, StationId(0), StationId(1), Direction::Forward);
        ResourceView::derive(&[stopped, moving]);
    }

    #[test]
    #[should_panic(expected = "station slot occupied and reserved")]
    fn resource_view_rejects_occupying_reserved_slot() {
        let stopped = Train::new(TrainId(1), 100, StationId(1), Direction::Forward);
        let moving = moving_train(2, StationId(0), StationId(1), Direction::Forward);
        ResourceView::derive(&[moving, stopped]);
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
            let mut forward = Train::new(TrainId(1), 100, StationId(1), Direction::Forward);
            forward.state = TrainState::AtStation {
                station: StationId(1),
                state,
            };
            let mut backward = forward.clone();
            backward.id = TrainId(2);
            backward.direction = Direction::Backward;
            let view = ResourceView::derive(&[forward, backward]);
            for (direction, id) in [
                (Direction::Forward, TrainId(1)),
                (Direction::Backward, TrainId(2)),
            ] {
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
        let view = ResourceView::derive(&[moving_train(
            1,
            StationId(0),
            StationId(1),
            Direction::Forward,
        )]);
        assert_eq!(
            view.track_occupants.get(&(StationId(0), StationId(1))),
            Some(&TrainId(1))
        );
        assert_eq!(
            view.station_reservations.get(&StationSlot {
                station: StationId(1),
                direction: Direction::Forward
            }),
            Some(&TrainId(1))
        );
        assert_eq!(view.track_occupants.len(), 1);
        assert_eq!(view.station_reservations.len(), 1);
        assert!(view.station_occupants.is_empty());
    }
}
