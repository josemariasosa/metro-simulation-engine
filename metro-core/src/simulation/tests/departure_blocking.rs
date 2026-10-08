use crate::command::{CommandError, TrainCommand};
use crate::domain::constraint::ConstraintView;
use crate::domain::departure::{can_admit_departure, select_departure_candidate};
use crate::domain::resource::ResourceView;
use crate::domain::train::TrainId;
use crate::simulation::Simulation;
use crate::simulation::tests::{SimulationFixture, committed_train_state};
use crate::{AtStationState, Direction, DwellPolicy, Train, TrainState};

fn assert_blocked_unchanged(simulation: &mut Simulation, train_id: TrainId) {
    let before = simulation.snapshot();
    let trains_before = committed_train_state(simulation);
    let resources_before = ResourceView::derive(simulation.trains());
    let next_train_id_before = simulation.next_train_id;
    let constraints_before = simulation.constraints.clone();
    let next_constraint_id_before = simulation.next_constraint_id;

    assert_eq!(
        simulation.apply_command(TrainCommand::Accelerate { train_id }),
        Err(CommandError::Blocked),
    );

    assert_eq!(simulation.snapshot(), before);
    assert_eq!(committed_train_state(simulation), trains_before);
    assert_eq!(ResourceView::derive(simulation.trains()), resources_before,);
    assert_eq!(simulation.next_train_id, next_train_id_before);
    assert_eq!(simulation.constraints, constraints_before);
    assert_eq!(simulation.next_constraint_id, next_constraint_id_before);
}

#[test]
fn occupied_track_and_reserved_destination_block_manual_departure_exactly() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 10);
    let train = Train::new_manual(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    let mut blocker = Train::moving_train(a, b, Direction::Forward);

    blocker.set_moving_for_test(a, b, 4);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();
    let _ = simulation.add_train(blocker).unwrap();

    let resources = ResourceView::derive(simulation.trains());

    assert!(!resources.track_available(a, b));
    assert!(!resources.station_slot_available(b, Direction::Forward));

    assert_blocked_unchanged(&mut simulation, train_id);
}

#[test]
fn occupied_destination_blocks_during_dwell_and_after_ready_exactly() {
    let SimulationFixture {
        network,
        stations: [a, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let train_id = simulation
        .add_train(Train::new_manual(
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ))
        .unwrap();

    simulation
        .add_train(Train::new_manual(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ))
        .unwrap();

    for elapsed in 0..=3 {
        assert_eq!(simulation.elapsed_seconds, elapsed);

        assert_blocked_unchanged(&mut simulation, train_id);

        if elapsed < 3 {
            simulation.step();
        }
    }

    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::AtStation {
            station: a,
            state: AtStationState::Ready,
        }
    );
}

#[test]
fn blocked_terminal_reversal_preserves_direction_until_fresh_command() {
    let SimulationFixture {
        network,
        stations: [a, b, c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let train_id = simulation
        .add_train(Train::new_manual(
            100,
            c,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ))
        .unwrap();

    let blocker_id = simulation
        .add_train(Train::new_manual(
            100,
            b,
            Direction::Backward,
            DwellPolicy::default_dwell_seconds(),
        ))
        .unwrap();

    simulation.step();

    assert_blocked_unchanged(&mut simulation, train_id);

    assert_eq!(simulation.train(train_id).direction(), Direction::Forward);

    simulation
        .apply_command(TrainCommand::Accelerate {
            train_id: blocker_id,
        })
        .unwrap();

    assert_eq!(
        simulation.train(blocker_id).state(),
        TrainState::Moving {
            from: b,
            to: a,
            elapsed_seconds: 0,
        }
    );

    simulation.step();

    assert_eq!(simulation.train(train_id).direction(), Direction::Forward);

    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::AtStation {
            station: c,
            state: AtStationState::Dwelling {
                elapsed_seconds: 2,
                dwell_seconds: 3,
            },
        }
    );

    simulation
        .apply_command(TrainCommand::Accelerate { train_id })
        .unwrap();

    assert_eq!(simulation.elapsed_seconds, 2);
    assert_eq!(simulation.train(train_id).direction(), Direction::Backward);
    assert_eq!(simulation.train(train_id).velocity(), 1);

    assert_eq!(
        simulation.train(train_id).state(),
        TrainState::Moving {
            from: c,
            to: b,
            elapsed_seconds: 0,
        }
    );

    ResourceView::derive(simulation.trains());
}

#[test]
fn physical_blocking_of_existing_track_never_selects_reverse_fallback() {
    let SimulationFixture {
        network,
        stations: [a, b, c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let train_id = simulation
        .add_train(Train::new_manual(
            100,
            b,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ))
        .unwrap();

    simulation
        .add_train(Train::moving_train(b, c, Direction::Forward))
        .unwrap();

    let resources = ResourceView::derive(simulation.trains());
    let constraints = ConstraintView::default();

    let reverse = select_departure_candidate(&simulation.network, b, Direction::Backward).unwrap();

    assert_eq!(reverse.to, a);
    assert!(can_admit_departure(&reverse, &resources, &constraints));

    assert_blocked_unchanged(&mut simulation, train_id);
}
