use crate::domain::train::{Direction, TrainState};
use crate::simulation::{TrainEntity, TrainId};
use crate::station::StationId;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StationSlot {
    pub(crate) station: StationId,
    pub(crate) direction: Direction,
}

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ResourceView {
    pub(crate) station_occupants: HashMap<StationSlot, TrainId>,
    pub(crate) station_reservations: HashMap<StationSlot, TrainId>,
    pub(crate) track_occupants: HashMap<(StationId, StationId), TrainId>,
}

impl ResourceView {
    pub(crate) fn track_available(&self, from: StationId, to: StationId) -> bool {
        !self.track_occupants.contains_key(&(from, to))
    }

    pub(crate) fn station_slot_available(&self, station: StationId, direction: Direction) -> bool {
        let slot = StationSlot { station, direction };
        !self.station_occupants.contains_key(&slot)
            && !self.station_reservations.contains_key(&slot)
    }

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

    pub(crate) fn derive(trains: &[TrainEntity]) -> Self {
        Self::derive_iter(trains.iter())
    }

    pub(crate) fn derive_iter<'a>(trains: impl IntoIterator<Item = &'a TrainEntity>) -> Self {
        let mut view = Self::default();
        for TrainEntity {
            id: train_id,
            train,
        } in trains
        {
            match train.state() {
                TrainState::AtStation { station, .. } => {
                    let slot = StationSlot {
                        station,
                        direction: train.direction(),
                    };
                    assert!(
                        !view.station_reservations.contains_key(&slot),
                        "station slot occupied and reserved"
                    );
                    assert!(
                        !view.station_occupants.contains_key(&slot),
                        "station slot already occupied"
                    );
                    view.station_occupants.insert(slot, *train_id);
                }
                TrainState::Moving { from, to, .. } => {
                    assert!(
                        !view.track_occupants.contains_key(&(from, to)),
                        "directed track already occupied"
                    );
                    view.track_occupants.insert((from, to), *train_id);
                    view.reserve_station(
                        StationSlot {
                            station: to,
                            direction: train.direction(),
                        },
                        *train_id,
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
    use crate::domain::train::{AtStationState, Train};
    use crate::dwell::DwellPolicy;
    use crate::network::Network;
    use crate::test_utils::utils::moving_train_entity;

    #[test]
    fn availability_checks_track_occupancy_and_both_slot_claims() {
        let a = StationId(0);
        let b = StationId(1);
        let empty = ResourceView::default();
        assert!(empty.track_available(a, b));
        assert!(empty.station_slot_available(b, Direction::Forward));

        let trains = &[TrainEntity {
            id: TrainId(1),
            train: Train::new(
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        }];
        let occupied = ResourceView::derive(trains);
        assert!(occupied.track_available(a, b));
        assert!(!occupied.station_slot_available(b, Direction::Forward));
        assert!(occupied.station_slot_available(b, Direction::Backward));

        let reserved = ResourceView::derive(&[moving_train_entity(1, a, b, Direction::Forward)]);
        assert!(!reserved.track_available(a, b));
        assert!(reserved.track_available(b, a));
        assert!(!reserved.station_slot_available(b, Direction::Forward));
        assert!(reserved.station_slot_available(b, Direction::Backward));
        assert!(reserved.station_slot_available(a, Direction::Forward));
    }

    #[test]
    fn admitted_departure_rederives_exact_ownership_including_reversal() {
        use crate::command::TrainCommand;
        use crate::dwell::DwellPolicy;
        use crate::simulation::Simulation;

        for reverse in [false, true] {
            let mut network = Network::new();
            let a = network.add_station("A");
            let b = network.add_station("B");
            network.connect_bidirectional(a, b, 2);
            let (from, to, direction) = if reverse {
                (b, a, Direction::Backward)
            } else {
                (a, b, Direction::Forward)
            };
            let train = Train::new_manual(
                100,
                from,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            );
            // A reversal must not require the opposite source slot to be free.
            let other = Train::new_manual(
                100,
                from,
                Direction::Backward,
                DwellPolicy::default_dwell_seconds(),
            );
            let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());
            let train_id = simulation.add_train(train);
            let other_id = simulation.add_train(other);
            let before = ResourceView::derive(simulation.trains());
            let source = StationSlot {
                station: from,
                direction: Direction::Forward,
            };
            assert_eq!(before.station_occupants.get(&source), Some(&train_id));

            simulation
                .apply_command(TrainCommand::Accelerate { train_id: train_id })
                .unwrap();

            assert_eq!(simulation.elapsed_seconds, 0);
            let TrainEntity { id: _, train } = &simulation.trains()[0];
            assert_eq!(train.direction(), direction);
            assert_eq!(train.velocity(), 1);
            assert_eq!(
                train.state(),
                TrainState::Moving {
                    from,
                    to,
                    elapsed_seconds: 0
                }
            );
            let after = ResourceView::derive(simulation.trains());
            assert!(!after.station_occupants.contains_key(&source));
            assert_eq!(
                after.station_occupants,
                HashMap::from([(
                    StationSlot {
                        station: from,
                        direction: Direction::Backward
                    },
                    other_id
                ),])
            );
            assert_eq!(
                after.track_occupants,
                HashMap::from([((from, to), train_id)])
            );
            assert_eq!(
                after.station_reservations,
                HashMap::from([(
                    StationSlot {
                        station: to,
                        direction
                    },
                    train_id
                ),])
            );
        }
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
            TrainEntity {
                id: TrainId(1),
                train: Train::new(
                    100,
                    b,
                    Direction::Forward,
                    DwellPolicy::default_dwell_seconds(),
                ),
            },
            TrainEntity {
                id: TrainId(2),
                train: Train::new(
                    100,
                    b,
                    Direction::Backward,
                    DwellPolicy::default_dwell_seconds(),
                ),
            },
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
        let train = Train::new(
            100,
            StationId(0),
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let first = TrainEntity {
            id: TrainId(1),
            train: train.clone(),
        };
        let other = TrainEntity {
            id: TrainId(2),
            train: train,
        };
        ResourceView::derive(&[first, other]);
    }

    #[test]
    #[should_panic(expected = "directed track already occupied")]
    fn resource_view_rejects_duplicate_track_occupants() {
        ResourceView::derive(&[
            moving_train_entity(1, StationId(0), StationId(1), Direction::Forward),
            moving_train_entity(2, StationId(0), StationId(1), Direction::Forward),
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
        let stopped = Train::new(
            100,
            StationId(1),
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let moving = moving_train_entity(2, StationId(0), StationId(1), Direction::Forward);
        ResourceView::derive(&[
            TrainEntity {
                id: TrainId(1),
                train: stopped,
            },
            moving,
        ]);
    }

    #[test]
    #[should_panic(expected = "station slot occupied and reserved")]
    fn resource_view_rejects_occupying_reserved_slot() {
        let stopped = TrainEntity {
            id: TrainId(1),
            train: Train::new(
                100,
                StationId(1),
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        };
        let moving = moving_train_entity(2, StationId(0), StationId(1), Direction::Forward);
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
            let mut forward = TrainEntity {
                id: TrainId(1),
                train: Train::new(
                    100,
                    StationId(1),
                    Direction::Forward,
                    DwellPolicy::default_dwell_seconds(),
                ),
            };
            match state {
                AtStationState::Ready => {
                    forward.train.set_ready_for_test(StationId(1));
                }
                AtStationState::Dwelling {
                    elapsed_seconds: _,
                    dwell_seconds,
                } => {
                    forward
                        .train
                        .set_dwell_for_test(StationId(1), dwell_seconds);
                }
            }

            let mut backward = TrainEntity {
                id: TrainId(2),
                train: forward.train.clone(),
            };
            backward.train.set_direction_for_test(Direction::Backward);
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
        let view = ResourceView::derive(&[moving_train_entity(
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
