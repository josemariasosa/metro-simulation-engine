use super::*;

#[test]
fn initial_dwelling_state_maps_to_snapshot_state() {
    let SimulationFixture {
        network,
        stations: [a, _],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 10);
    let train = Train::new(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );
    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    let train = simulation.train(train_id).unwrap();

    let TrainState::AtStation {
        station,
        state:
            AtStationState::Dwelling {
                elapsed_seconds,
                dwell_seconds,
            },
    } = train.state()
    else {
        panic!("expected train to start dwelling");
    };

    let snapshot_state = dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds);

    assert_eq!(
        snapshot_state,
        TrainSnapshotState::Dwelling {
            station: StationId(0),
            remaining_seconds: 3,
        }
    );
}

#[test]
fn dwelling_snapshot_countdown_matches_future_steps_until_departure() {
    let SimulationFixture {
        network,
        stations: [a, _],
        dwell_policy,
    } = SimulationFixture::new(["A", "B"], 10);

    let train = Train::new(
        100,
        a,
        Direction::Forward,
        DwellPolicy::default_dwell_seconds(),
    );

    let mut simulation = Simulation::new(network, vec![], dwell_policy);
    let train_id = simulation.add_train(train).unwrap();

    for remaining_seconds in [3, 2, 1] {
        let snapshot = simulation.snapshot();
        let train = Simulation::snapshot_train_by_id(&snapshot, train_id);

        assert_eq!(
            train.state,
            TrainSnapshotState::Dwelling {
                station: a,
                remaining_seconds,
            }
        );

        simulation.step();
    }

    assert!(matches!(
        simulation.train(train_id).unwrap().state(),
        TrainState::Moving { .. }
    ));
}

#[test]
fn dwelling_snapshot_uses_stored_dwell_duration() {
    let snapshot = dwelling_snapshot_state(StationId(0), 2, 5);

    assert_eq!(
        snapshot,
        TrainSnapshotState::Dwelling {
            station: StationId(0),
            remaining_seconds: 3,
        }
    );
}

#[test]
fn snapshot_sorts_constraint_ids_without_reordering_registry() {
    let (mut simulation, [a, b, _]) = constraint_simulation();
    let first = simulation
        .create_constraint_at(closed(a), 5, None, ConstraintOrigin::Planned)
        .unwrap();
    let second = simulation
        .create_constraint_at(closed(b), 6, None, ConstraintOrigin::Injected)
        .unwrap();
    let third = simulation
        .create_constraint_at(closed(a), 7, Some(9), ConstraintOrigin::Planned)
        .unwrap();
    simulation.constraints.reverse();
    let registry_before = simulation.constraints.clone();

    let snapshot = simulation.snapshot();

    assert_eq!(
        snapshot
            .constraints
            .iter()
            .map(|constraint| constraint.id)
            .collect::<Vec<_>>(),
        vec![first, second, third]
    );
    assert_eq!(simulation.constraints, registry_before);
}

#[test]
fn snapshot_omits_stale_expired_record_without_pruning_registry() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    simulation
        .create_constraint_at(closed(a), 5, Some(7), ConstraintOrigin::Planned)
        .unwrap();
    simulation.elapsed_seconds = 7;
    let registry_before = simulation.constraints.clone();

    assert!(simulation.snapshot().constraints.is_empty());
    assert_eq!(simulation.constraints, registry_before);
}
