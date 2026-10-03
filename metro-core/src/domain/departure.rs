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
) -> bool {
    resources.track_available(candidate.from, candidate.to)
        && resources.station_slot_available(candidate.to, candidate.direction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::utils::moving_train;

    // Synthetic claims isolate each guard; real moving trains claim both resources.
    fn candidate() -> DepartureCandidate {
        DepartureCandidate {
            from: StationId(0),
            to: StationId(1),
            direction: Direction::Forward,
        }
    }

    #[test]
    fn reserved_destination_alone_rejects_departure() {
        let candidate = candidate();
        let reservation = moving_train(StationId(2), candidate.to, candidate.direction);
        let resources = ResourceView::derive([(1, &reservation)]);

        assert!(resources.track_available(candidate.from, candidate.to));
        assert!(!resources.station_slot_available(candidate.to, candidate.direction));
        assert!(!can_admit_departure(&candidate, &resources));
    }

    #[test]
    fn occupied_track_alone_rejects_departure() {
        let candidate = candidate();
        let track_occupant =
            moving_train(candidate.from, candidate.to, candidate.direction.reverse());
        let resources = ResourceView::derive([(1, &track_occupant)]);

        assert!(!resources.track_available(candidate.from, candidate.to));
        assert!(resources.station_slot_available(candidate.to, candidate.direction));
        assert!(!can_admit_departure(&candidate, &resources));
    }
}
