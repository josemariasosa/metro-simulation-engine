use crate::train::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StationId(pub usize);

#[derive(Debug)]
pub struct Station {
    pub id: StationId,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct Network {
    stations: Vec<Station>,
}

impl Network {
    pub fn new() -> Self {
        Self {
            stations: Vec::new(),
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

    pub fn next_station(&self, current: StationId, direction: Direction) -> Option<StationId> {
        let index = current.0;
        match direction {
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
        }
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
    fn finds_next_station_in_both_directions() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");
        let c = network.add_station("C");
        let _ = network.add_station("D");
        let e = network.add_station("E");

        assert_eq!(network.next_station(b, Direction::Forward), Some(c));

        assert_eq!(network.next_station(b, Direction::Backward), Some(a));

        assert_eq!(network.next_station(e, Direction::Forward), None);

        assert_eq!(network.next_station(a, Direction::Backward), None);
    }
}
