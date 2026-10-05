use crate::domain::constraint::ConstraintView;
use crate::domain::network::Network;
use crate::domain::resource::ResourceView;
use crate::domain::station::StationId;
use crate::domain::train::Direction;

pub(crate) struct DepartureCandidate {
    pub(crate) from: StationId,
    pub(crate) to: StationId,
    pub(crate) direction: Direction,
}

pub(crate) fn select_departure_candidate(
    network: &Network,
    station: StationId,
    direction: Direction,
) -> Option<DepartureCandidate> {
    network
        .next_track(station, direction)
        .map(|track| DepartureCandidate {
            from: station,
            to: track.to,
            direction,
        })
        .or_else(|| {
            let direction = direction.reverse();
            network
                .next_track(station, direction)
                .map(|track| DepartureCandidate {
                    from: station,
                    to: track.to,
                    direction,
                })
        })
}

pub(crate) fn can_admit_departure<Owner: Copy>(
    candidate: &DepartureCandidate,
    resources: &ResourceView<Owner>,
    constraints: &ConstraintView,
) -> bool {
    resources.track_available(candidate.from, candidate.to)
        && resources.station_slot_available(candidate.to, candidate.direction)
        && constraints.permits_departure(candidate.from, candidate.to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::constraint::OperationalConstraint;
    use crate::domain::dwell::DwellPolicy;
    use crate::domain::network::Network;
    use crate::domain::resource::ResourceView;
    use crate::domain::train::{Direction, Train, TrainState};
    use crate::test_utils::utils::moving_train;

    // Synthetic claims isolate each guard; real moving trains claim both resources.
    fn candidate() -> DepartureCandidate {
        DepartureCandidate {
            from: StationId(0),
            to: StationId(1),
            direction: Direction::Forward,
        }
    }

    fn candidate_between(from: StationId, to: StationId) -> DepartureCandidate {
        DepartureCandidate {
            from,
            to,
            direction: if to.0 > from.0 {
                Direction::Forward
            } else {
                Direction::Backward
            },
        }
    }

    #[test]
    fn empty_constraints_admit_physically_available_departure() {
        let candidate = candidate();
        let resources = ResourceView::<usize>::default();
        let constraints = ConstraintView::default();

        assert!(resources.track_available(candidate.from, candidate.to));
        assert!(resources.station_slot_available(candidate.to, candidate.direction));
        assert!(can_admit_departure(&candidate, &resources, &constraints));
    }

    #[test]
    fn exact_directed_track_constraint_does_not_block_reverse_edge() {
        let a = StationId(0);
        let b = StationId(1);
        let forward = candidate_between(a, b);
        let reverse = candidate_between(b, a);
        let resources = ResourceView::<usize>::default();
        let constraints =
            ConstraintView::from_constraints([OperationalConstraint::TrackUnavailable {
                from: a,
                to: b,
            }]);

        assert!(can_admit_departure(&reverse, &resources, &constraints));
        assert!(!can_admit_departure(&forward, &resources, &constraints));
    }

    #[test]
    fn blocked_departures_restrict_the_source_not_the_destination() {
        let a = StationId(0);
        let b = StationId(1);
        let from_a = candidate_between(a, b);
        let from_b = candidate_between(b, a);
        let resources = ResourceView::<usize>::default();
        let constraints =
            ConstraintView::from_constraints([OperationalConstraint::StationDeparturesBlocked {
                station: b,
            }]);

        assert!(can_admit_departure(&from_a, &resources, &constraints));
        assert!(!can_admit_departure(&from_b, &resources, &constraints));
    }

    #[test]
    fn unavailable_station_blocks_departures_from_and_into_it() {
        let a = StationId(0);
        let b = StationId(1);
        let from_a = candidate_between(a, b);
        let from_b = candidate_between(b, a);
        let resources = ResourceView::<usize>::default();
        let constraints =
            ConstraintView::from_constraints([OperationalConstraint::StationUnavailable {
                station: b,
            }]);

        assert!(!can_admit_departure(&from_a, &resources, &constraints));
        assert!(!can_admit_departure(&from_b, &resources, &constraints));
    }

    #[test]
    fn matching_and_unrelated_constraints_compose_as_blocking_or() {
        let candidate = candidate();
        let resources = ResourceView::<usize>::default();
        let constraints = ConstraintView::from_constraints([
            OperationalConstraint::TrackUnavailable {
                from: candidate.from,
                to: candidate.to,
            },
            OperationalConstraint::StationUnavailable {
                station: StationId(2),
            },
        ]);

        assert!(!can_admit_departure(&candidate, &resources, &constraints));
    }

    #[test]
    fn physical_and_operational_denials_both_reject_shared_admission() {
        let candidate = candidate();
        let unrelated_constraints =
            ConstraintView::from_constraints([OperationalConstraint::StationUnavailable {
                station: StationId(2),
            }]);
        let track_occupant =
            moving_train(candidate.from, candidate.to, candidate.direction.reverse());
        let physically_denied = ResourceView::derive([(1, &track_occupant)]);

        assert!(unrelated_constraints.permits_departure(candidate.from, candidate.to));
        assert!(!can_admit_departure(
            &candidate,
            &physically_denied,
            &unrelated_constraints
        ));

        let physically_available = ResourceView::<usize>::default();
        let operationally_denied =
            ConstraintView::from_constraints([OperationalConstraint::TrackUnavailable {
                from: candidate.from,
                to: candidate.to,
            }]);
        assert!(physically_available.track_available(candidate.from, candidate.to));
        assert!(physically_available.station_slot_available(candidate.to, candidate.direction));
        assert!(!can_admit_departure(
            &candidate,
            &physically_available,
            &operationally_denied
        ));
    }

    #[test]
    fn operational_block_rejects_selected_edge_without_reverse_fallback() {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");
        network.connect_bidirectional(a, b, 2);
        network.connect_bidirectional(b, c, 2);
        let train = Train::new(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let selected = select_departure_candidate(&network, b, Direction::Forward).unwrap();
        let constraints =
            ConstraintView::from_constraints([OperationalConstraint::TrackUnavailable {
                from: b,
                to: c,
            }]);
        let resources = ResourceView::<usize>::default();

        assert_eq!(
            (selected.from, selected.to, selected.direction),
            (b, c, Direction::Forward)
        );
        assert!(!can_admit_departure(&selected, &resources, &constraints));
        assert_eq!(train.direction(), Direction::Forward);
        assert!(matches!(train.state(), TrainState::AtStation { station, .. } if station == b));
    }

    #[test]
    fn blocked_terminal_reversal_candidate_does_not_change_train_direction() {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        network.connect_bidirectional(a, b, 2);
        let train = Train::new(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let selected = select_departure_candidate(&network, b, Direction::Forward).unwrap();
        let constraints =
            ConstraintView::from_constraints([OperationalConstraint::TrackUnavailable {
                from: b,
                to: a,
            }]);
        let resources = ResourceView::<usize>::default();

        assert_eq!(
            (selected.from, selected.to, selected.direction),
            (b, a, Direction::Backward)
        );
        assert!(!can_admit_departure(&selected, &resources, &constraints));
        assert_eq!(train.direction(), Direction::Forward);
        assert!(matches!(train.state(), TrainState::AtStation { station, .. } if station == b));
    }

    #[test]
    fn reserved_destination_alone_rejects_departure() {
        let candidate = candidate();
        let reservation = moving_train(StationId(2), candidate.to, candidate.direction);
        let resources = ResourceView::derive([(1, &reservation)]);
        let constraints = ConstraintView::default();

        assert!(resources.track_available(candidate.from, candidate.to));
        assert!(!resources.station_slot_available(candidate.to, candidate.direction));
        assert!(!can_admit_departure(&candidate, &resources, &constraints));
    }

    #[test]
    fn occupied_track_alone_rejects_departure() {
        let candidate = candidate();
        let track_occupant =
            moving_train(candidate.from, candidate.to, candidate.direction.reverse());
        let resources = ResourceView::derive([(1, &track_occupant)]);
        let constraints = ConstraintView::default();

        assert!(!resources.track_available(candidate.from, candidate.to));
        assert!(resources.station_slot_available(candidate.to, candidate.direction));
        assert!(!can_admit_departure(&candidate, &resources, &constraints));
    }
}
