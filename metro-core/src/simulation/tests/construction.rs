use crate::{
    Direction, DwellPolicy, Train,
    simulation::{Simulation, tests::SimulationFixture},
};

#[test]
#[should_panic(expected = "invalid initial train registration: ResourceConflict")]
fn simulation_rejects_duplicate_station_slot_occupancy() {
    let SimulationFixture {
        network,
        stations: [_a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);
    let trains = vec![
        Train::new(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ),
        Train::new(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ),
    ];
    Simulation::new(network, trains, dwell_policy);
}

#[test]
#[should_panic(expected = "invalid initial train registration: ResourceConflict")]
fn simulation_rejects_duplicate_directed_track_occupancy() {
    let SimulationFixture {
        network,
        stations: [a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);
    let trains = vec![
        Train::moving_train(a, b, Direction::Forward),
        Train::moving_train(a, b, Direction::Forward),
    ];
    Simulation::new(network, trains, dwell_policy);
}

#[test]
#[should_panic(expected = "invalid initial train registration: ResourceConflict")]
fn simulation_rejects_occupied_and_reserved_slot() {
    let SimulationFixture {
        network,
        stations: [a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);
    let trains = vec![
        Train::new(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ),
        Train::moving_train(a, b, Direction::Forward),
    ];
    Simulation::new(network, trains, dwell_policy);
}

#[test]
#[should_panic(expected = "invalid initial train registration: ResourceConflict")]
fn simulation_rejects_reserved_and_occupied_slot() {
    let SimulationFixture {
        network,
        stations: [a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);
    let trains = vec![
        Train::moving_train(a, b, Direction::Forward),
        Train::new(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ),
    ];
    Simulation::new(network, trains, dwell_policy);
}

#[test]
fn simulation_accepts_opposite_station_slots() {
    let SimulationFixture {
        network,
        stations: [_a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);
    let trains = vec![
        Train::new(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ),
        Train::new(
            100,
            b,
            Direction::Backward,
            DwellPolicy::default_dwell_seconds(),
        ),
    ];
    Simulation::new(network, trains, dwell_policy);
}

#[test]
fn simulation_accepts_opposite_directed_tracks() {
    let SimulationFixture {
        network,
        stations: [a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);
    let trains = vec![
        Train::moving_train(a, b, Direction::Forward),
        Train::moving_train(b, a, Direction::Backward),
    ];
    Simulation::new(network, trains, dwell_policy);
}

#[test]
fn simulation_accepts_opposite_destination_reservations() {
    let SimulationFixture {
        network,
        stations: [a, b, c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);
    let trains = vec![
        Train::moving_train(a, b, Direction::Forward),
        Train::moving_train(c, b, Direction::Backward),
    ];
    Simulation::new(network, trains, dwell_policy);
}
