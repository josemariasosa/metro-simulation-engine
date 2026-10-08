mod enforcement;
mod lifecycle;
mod registry;
mod scheduling;
mod validation;

use super::*;

fn world(manual: bool, station: usize, direction: Direction, dwell: u64) -> (Simulation, TrainId) {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");

    network.connect_bidirectional(a, b, 3);
    network.connect_bidirectional(b, c, 3);

    let constructor = if manual {
        Train::new_manual
    } else {
        Train::new
    };

    let train = constructor(100, StationId(station), direction, dwell);

    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());

    let train_id = simulation.add_train(train).unwrap();

    (simulation, train_id)
}

fn closure(station: usize) -> OperationalConstraint {
    OperationalConstraint::StationUnavailable {
        station: StationId(station),
    }
}

fn schedule(
    s: &mut Simulation,
    value: OperationalConstraint,
    start: u64,
    end: Option<u64>,
) -> ConstraintId {
    s.create_constraint_at(value, start, end, ConstraintOrigin::Planned)
        .unwrap()
}

fn command_const(simulation: &mut Simulation, train_id: TrainId) -> Result<(), CommandError> {
    simulation.apply_command(TrainCommand::Accelerate { train_id })
}

fn moving_const(simulation: &Simulation, train_id: TrainId) -> bool {
    matches!(
        simulation.train(train_id).state(),
        TrainState::Moving { .. }
    )
}

fn claims(s: &Simulation) -> ResourceView<TrainId> {
    ResourceView::derive(s.trains())
}

fn assert_exact_command(
    simulation: &mut Simulation,
    train_id: TrainId,
    expected: Result<(), CommandError>,
) {
    let snapshot = simulation.snapshot();
    let trains = committed_train_state(simulation);
    let resources = claims(simulation);
    let next_train_id = simulation.next_train_id;
    let constraints = simulation.constraints.clone();
    let next_constraint_id = simulation.next_constraint_id;

    assert_eq!(command_const(simulation, train_id), expected);

    assert_eq!(simulation.snapshot(), snapshot);
    assert_eq!(committed_train_state(simulation), trains);
    assert_eq!(claims(simulation), resources);
    assert_eq!(simulation.next_train_id, next_train_id);
    assert_eq!(simulation.constraints, constraints);
    assert_eq!(simulation.next_constraint_id, next_constraint_id);
}

fn check_action(value: OperationalConstraint, station: usize, direction: Direction, allowed: bool) {
    for manual in [false, true] {
        let (mut simulation, train_id) = world(manual, station, direction, 2);

        schedule(&mut simulation, value, 1, None);
        simulation.step();

        let before_direction = simulation.train(train_id).direction();

        if manual {
            assert_eq!(
                command_const(&mut simulation, train_id),
                if allowed {
                    Ok(())
                } else {
                    Err(CommandError::Blocked)
                }
            );
        } else {
            simulation.step();
        }

        assert_eq!(moving_const(&simulation, train_id), allowed);

        if !allowed {
            let train = simulation.train(train_id);

            assert_eq!(train.direction(), before_direction);

            assert!(matches!(
                train.state(),
                TrainState::AtStation { station: at, .. }
                    if at == StationId(station)
            ));
        }
    }
}
