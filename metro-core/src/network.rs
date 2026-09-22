use std::collections::HashMap;

use crate::station::{Station, StationId};
use crate::train::Direction;

#[derive(Debug)]
pub struct Track {
    pub from: StationId,
    pub to: StationId,
    pub travel_seconds: u64,
}

#[derive(Debug, Default)]
pub struct Network {
    stations: Vec<Station>,
    tracks: HashMap<(StationId, StationId), Track>,
}

impl Network {
    pub fn new() -> Self {
        Self {
            stations: Vec::new(),
            tracks: HashMap::new(),
        }
    }

    pub fn add_station(&mut self, name: &str) -> StationId {
        let id = StationId(self.stations.len());
        let station = Station {
            id,
            name: name.to_string(),
        };
        self.stations.push(station);
        id
    }

    pub fn station_count(&self) -> usize {
        self.stations.len()
    }

    pub fn add_track(&mut self, from: StationId, to: StationId, travel_seconds: u64) {
        let track = Track {
            from,
            to,
            travel_seconds,
        };
        self.tracks.insert((from, to), track);
    }

    pub fn track(&self, from: StationId, to: StationId) -> Option<&Track> {
        self.tracks.get(&(from, to))
    }

    pub fn connect_bidirectional(&mut self, from: StationId, to: StationId, travel_seconds: u64) {
        self.add_track(from, to, travel_seconds);

        // Bidirectional connection
        self.add_track(to, from, travel_seconds);
    }

    pub fn next_track(&self, current: StationId, direction: Direction) -> Option<&Track> {
        let index = current.0;
        let next_station = match direction {
            Direction::Forward => {
                if index + 1 < self.stations.len() {
                    Some(StationId(index + 1))
                } else {
                    None
                }
            }
            Direction::Backward => {
                if index > 0 {
                    Some(StationId(index - 1))
                } else {
                    None
                }
            }
        };

        if let Some(next_station) = next_station {
            if let Some(track) = self.tracks.get(&(current, next_station)) {
                return Some(track);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::train::Direction;

    #[test]
    fn can_add_stations_to_network() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        assert_ne!(a, b);
        assert_eq!(network.station_count(), 2);
    }

    #[test]
    fn can_add_track() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.add_track(a, b, 180);

        assert_eq!(
            network
                .next_track(a, Direction::Forward)
                .unwrap()
                .travel_seconds,
            180
        );
        assert!(network.next_track(b, Direction::Backward).is_none());
    }

    #[test]
    fn station_connections_are_bidirectional() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 180);

        assert_eq!(
            network
                .next_track(a, Direction::Forward)
                .unwrap()
                .travel_seconds,
            180
        );
        assert_eq!(
            network
                .next_track(b, Direction::Backward)
                .unwrap()
                .travel_seconds,
            180
        );
    }

    #[test]
    fn returns_none_when_stations_are_not_connected() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");
        let _c = network.add_station("C");

        network.connect_bidirectional(a, b, 180);

        assert!(network.next_track(b, Direction::Forward).is_none());
    }

    #[test]
    fn next_track_returns_track_in_forward_direction() {
        let mut network = Network::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_bidirectional(station_a, station_b, 60);

        let track = network
            .next_track(station_a, Direction::Forward)
            .expect("expected track from A to B");

        assert_eq!(track.from, station_a);
        assert_eq!(track.to, station_b);
        assert_eq!(track.travel_seconds, 60);
    }

    #[test]
    fn next_track_returns_none_at_end_of_direction() {
        let mut network = Network::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_bidirectional(station_a, station_b, 60);

        let track = network.next_track(station_b, Direction::Forward);

        assert!(track.is_none());
    }

    #[test]
    fn next_track_returns_track_in_backward_direction() {
        let mut network = Network::new();

        let station_a = network.add_station("A");
        let station_b = network.add_station("B");

        network.connect_bidirectional(station_a, station_b, 60);

        let track = network
            .next_track(station_b, Direction::Backward)
            .expect("expected track from B to A");

        assert_eq!(track.from, station_b);
        assert_eq!(track.to, station_a);
        assert_eq!(track.travel_seconds, 60);
    }

    #[test]
    fn track_returns_exact_connection() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        network.connect_bidirectional(a, b, 180);

        let track = network.track(a, b).expect("expected track from A to B");

        assert_eq!(track.from, a);
        assert_eq!(track.to, b);
        assert_eq!(track.travel_seconds, 180);
    }
}
