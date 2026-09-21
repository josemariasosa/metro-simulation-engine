use crate::station::Network;
use crate::train::Train;

#[derive(Debug)]
pub struct Simulation {
    pub elapsed_seconds: u64,
    pub network: Network,
    pub trains: Vec<Train>,
}

impl Simulation {
    pub fn new(network: Network, trains: Vec<Train>) -> Self {
        Self {
            elapsed_seconds: 0,
            network,
            trains,
        }
    }

    pub fn step(&mut self) {
        self.elapsed_seconds += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_advances_time() {
        let network = Network::new();
        let trains = Vec::new();

        let mut simulation = Simulation::new(network, trains);

        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.elapsed_seconds, 3);
    }
}
