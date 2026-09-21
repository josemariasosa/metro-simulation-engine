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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_add_stations_to_network() {
        let mut network = Network::new();

        let a = network.add_station("A");
        let b = network.add_station("B");

        assert_ne!(a, b);
        assert_eq!(network.station_count(), 2);
    }
}
