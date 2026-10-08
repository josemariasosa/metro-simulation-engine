use crate::command::{CommandError, TrainCommand};
use crate::domain::resource::ResourceView;
use crate::domain::train::{Direction, Train, TrainId};
use crate::simulation::tests::{SimulationFixture, committed_train_state};
use crate::simulation::{AddTrainError, Simulation};
use crate::{AtStationState, DwellPolicy, TrainState};

#[test]
fn add_train_allocates_monotonic_ids() {
    let SimulationFixture {
        network,
        stations: [a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let first_id = simulation
        .add_train(Train::new_manual(100, a, Direction::Forward, 3))
        .unwrap();

    let second_id = simulation
        .add_train(Train::new_manual(200, b, Direction::Forward, 3))
        .unwrap();

    assert_eq!(first_id, TrainId(0));
    assert_eq!(second_id, TrainId(1));
}

#[test]
fn train_identity_is_independent_of_storage_order() {
    let SimulationFixture {
        network,
        stations: [a, b, c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let first_id = simulation
        .add_train(Train::new_manual(100, a, Direction::Forward, 3))
        .unwrap();

    let second_id = simulation
        .add_train(Train::new_manual(200, b, Direction::Forward, 3))
        .unwrap();

    simulation.trains.reverse();

    assert_eq!(simulation.train(first_id).unwrap().capacity(), 100);
    assert_eq!(simulation.train(second_id).unwrap().capacity(), 200);

    simulation
        .apply_command(TrainCommand::Accelerate {
            train_id: second_id,
        })
        .unwrap();

    assert_eq!(
        simulation.train(second_id).unwrap().state(),
        TrainState::Moving {
            from: b,
            to: c,
            elapsed_seconds: 0,
        }
    );

    assert_eq!(
        simulation.train(first_id).unwrap().state(),
        TrainState::AtStation {
            station: a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 3,
            },
        }
    );
}

#[test]
fn removed_train_ids_are_not_reused() {
    let SimulationFixture {
        network,
        stations: [a, b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let first_id = simulation
        .add_train(Train::new_manual(100, a, Direction::Forward, 3))
        .unwrap();

    let second_id = simulation
        .add_train(Train::new_manual(200, b, Direction::Forward, 3))
        .unwrap();

    simulation.trains.retain(|entity| entity.id() != first_id);

    assert_eq!(
        simulation.apply_command(TrainCommand::Accelerate { train_id: first_id }),
        Err(CommandError::UnknownTrain)
    );

    let third_id = simulation
        .add_train(Train::new_manual(300, a, Direction::Backward, 3))
        .unwrap();

    assert_eq!(third_id, TrainId(2));

    simulation.trains.clear();

    let fourth_id = simulation
        .add_train(Train::new_manual(400, a, Direction::Forward, 3))
        .unwrap();

    assert_eq!(fourth_id, TrainId(3));

    // Existing allocated IDs are never recycled just because storage shrinks.
    assert_ne!(third_id, first_id);
    assert_ne!(fourth_id, first_id);
    assert_ne!(fourth_id, second_id);
}

#[test]
fn failed_duplicate_registration_preserves_state_and_allocator() {
    let SimulationFixture {
        network,
        stations: [a, _],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 10);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    let train_id = simulation
        .add_train(Train::new(
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        ))
        .unwrap();

    let before = simulation.snapshot();
    let trains_before = committed_train_state(&simulation);
    let resources_before = ResourceView::derive(simulation.trains());
    let next_train_id_before = simulation.next_train_id;
    let constraints_before = simulation.constraints.clone();
    let next_constraint_id_before = simulation.next_constraint_id;
    let duplicate = simulation.train(train_id).unwrap().clone();

    assert_eq!(
        simulation.add_train(duplicate),
        Err(AddTrainError::ResourceConflict),
    );

    assert_eq!(simulation.snapshot(), before);
    assert_eq!(committed_train_state(&simulation), trains_before);
    assert_eq!(ResourceView::derive(simulation.trains()), resources_before);
    assert_eq!(simulation.next_train_id, next_train_id_before);
    assert_eq!(simulation.constraints, constraints_before);
    assert_eq!(simulation.next_constraint_id, next_constraint_id_before);

    let next_id = simulation
        .add_train(Train::new_manual(100, a, Direction::Backward, 3))
        .unwrap();

    assert_eq!(next_id, TrainId(1));
}

#[test]
fn failed_registration_at_id_exhaustion_preserves_state_and_allocator() {
    let SimulationFixture {
        network,
        stations: [_, b],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 10);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);

    simulation.next_train_id = usize::MAX;

    let before = simulation.snapshot();
    let trains_before = committed_train_state(&simulation);
    let resources_before = ResourceView::derive(simulation.trains());
    let constraints_before = simulation.constraints.clone();
    let next_constraint_id_before = simulation.next_constraint_id;

    assert_eq!(
        simulation.add_train(Train::new_manual(100, b, Direction::Forward, 3)),
        Err(AddTrainError::IdExhausted),
    );

    assert_eq!(simulation.snapshot(), before);
    assert_eq!(committed_train_state(&simulation), trains_before);
    assert_eq!(ResourceView::derive(simulation.trains()), resources_before);
    assert_eq!(simulation.next_train_id, usize::MAX);
    assert_eq!(simulation.constraints, constraints_before);
    assert_eq!(simulation.next_constraint_id, next_constraint_id_before);
}

#[test]
fn train_lookup_returns_none_for_unknown_id() {
    let SimulationFixture {
        network,
        dwell_policy,
        ..
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let simulation = Simulation::new(network, vec![], dwell_policy);

    assert!(simulation.train(TrainId(99)).is_none());
}

#[test]
fn zero_dwell_train_snapshots_without_panic() {
    let SimulationFixture {
        network,
        stations: [a, _b, _c],
        dwell_policy,
    } = SimulationFixture::new(["A", "B", "C"], 2);

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    simulation
        .add_train(Train::new_manual(0, a, Direction::Forward, 3))
        .unwrap();

    let snapshot = simulation.snapshot();
    assert_eq!(snapshot.trains.len(), 1);
}
