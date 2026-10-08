use crate::{DwellPolicy, Network, simulation::Simulation};

#[test]
fn step_advances_time_by_one_second() {
    let mut simulation = Simulation::new(Network::new(), vec![], DwellPolicy::new());

    assert_eq!(simulation.elapsed_seconds, 0);

    simulation.step();
    assert_eq!(simulation.elapsed_seconds, 1);

    simulation.step();
    assert_eq!(simulation.elapsed_seconds, 2);

    simulation.step();
    assert_eq!(simulation.elapsed_seconds, 3);
}
