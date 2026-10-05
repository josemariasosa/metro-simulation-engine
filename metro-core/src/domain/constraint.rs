use std::collections::HashSet;

use crate::domain::station::StationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationalConstraint {
    TrackUnavailable { from: StationId, to: StationId },
    StationDeparturesBlocked { station: StationId },
    StationUnavailable { station: StationId },
}

struct RestrictionView {
    unavailable_tracks: HashSet<(StationId, StationId)>,
    blocked_departure_stations: HashSet<StationId>,
    unavailable_stations: HashSet<StationId>,
}

impl RestrictionView {
    fn from_constraints(constraints: impl IntoIterator<Item = OperationalConstraint>) -> Self {
        let mut unavailable_tracks = HashSet::new();
        let mut blocked_departure_stations = HashSet::new();
        let mut unavailable_stations = HashSet::new();

        for constraint in constraints {
            match constraint {
                OperationalConstraint::TrackUnavailable { from, to } => {
                    unavailable_tracks.insert((from, to));
                }
                OperationalConstraint::StationDeparturesBlocked { station } => {
                    blocked_departure_stations.insert(station);
                }
                OperationalConstraint::StationUnavailable { station } => {
                    unavailable_stations.insert(station);
                }
            }
        }

        Self {
            unavailable_tracks,
            blocked_departure_stations,
            unavailable_stations,
        }
    }

    fn track_unavailable(&self, from: StationId, to: StationId) -> bool {
        self.unavailable_tracks.contains(&(from, to))
    }

    fn station_departures_blocked(&self, station: StationId) -> bool {
        self.blocked_departure_stations.contains(&station)
    }

    fn station_unavailable(&self, station: StationId) -> bool {
        self.unavailable_stations.contains(&station)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_unavailable_blocks_only_exact_directed_track() {
        let a = StationId(0);
        let b = StationId(1);

        let view = RestrictionView::from_constraints([OperationalConstraint::TrackUnavailable {
            from: a,
            to: b,
        }]);

        assert!(view.track_unavailable(a, b));
        assert!(!view.track_unavailable(b, a));
    }

    #[test]
    fn station_departures_blocked_blocks_that_source_station() {
        let a = StationId(0);
        let b = StationId(1);

        let view =
            RestrictionView::from_constraints([OperationalConstraint::StationDeparturesBlocked {
                station: b,
            }]);

        assert!(view.station_departures_blocked(b));
        assert!(!view.station_departures_blocked(a));
    }

    #[test]
    fn station_unavailable_marks_only_that_station() {
        let a = StationId(0);
        let b = StationId(1);

        let view = RestrictionView::from_constraints([OperationalConstraint::StationUnavailable {
            station: b,
        }]);

        assert!(view.station_unavailable(b));
        assert!(!view.station_unavailable(a));
    }

    #[test]
    fn multiple_constraints_compose_without_overwriting_each_other() {
        let a = StationId(0);
        let b = StationId(1);
        let c = StationId(2);

        let view = RestrictionView::from_constraints([
            OperationalConstraint::TrackUnavailable { from: a, to: b },
            OperationalConstraint::StationDeparturesBlocked { station: b },
            OperationalConstraint::StationUnavailable { station: c },
        ]);

        assert!(view.track_unavailable(a, b));
        assert!(view.station_departures_blocked(b));
        assert!(view.station_unavailable(c));
    }

    #[test]
    fn empty_restriction_view_contains_no_restrictions() {
        let a = StationId(0);
        let b = StationId(1);

        let view = RestrictionView::from_constraints([]);

        assert!(!view.track_unavailable(a, b));
        assert!(!view.station_departures_blocked(a));
        assert!(!view.station_unavailable(a));
    }
}
