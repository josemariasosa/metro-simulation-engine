use super::*;

#[test]
fn removal_cancels_scheduled_and_active_records_without_reusing_ids() {
    let (mut simulation, [a, b, _]) = constraint_simulation();
    let scheduled = simulation
        .create_constraint_at(closed(a), 10, None, ConstraintOrigin::Planned)
        .unwrap();
    let active = simulation
        .create_constraint_at(closed(b), 1, None, ConstraintOrigin::Injected)
        .unwrap();
    simulation.step();
    let trains_before = simulation.snapshot();

    assert_eq!(simulation.remove_constraint(scheduled), Ok(()));
    assert_eq!(simulation.remove_constraint(active), Ok(()));

    assert!(simulation.constraints.is_empty());
    assert_eq!(simulation.snapshot().trains, trains_before.trains);
    assert!(simulation.snapshot().constraints.is_empty());
    assert_eq!(
        simulation.remove_constraint(scheduled),
        Err(ConstraintError::UnknownConstraint)
    );
    assert_eq!(
        simulation.remove_constraint(active),
        Err(ConstraintError::UnknownConstraint)
    );
    assert_eq!(
        simulation.create_constraint_at(closed(a), 10, None, ConstraintOrigin::Planned),
        Ok(ConstraintId(2))
    );
}

#[test]
fn unknown_removal_preserves_registry_and_state() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    simulation
        .create_constraint_at(closed(a), 5, None, ConstraintOrigin::Planned)
        .unwrap();
    let snapshot = simulation.snapshot();
    let trains = committed_train_state(&simulation);
    let resources = ResourceView::derive(simulation.trains());
    let next_train_id = simulation.next_train_id;
    let records = simulation.constraints.clone();

    assert_eq!(
        simulation.remove_constraint(ConstraintId(7)),
        Err(ConstraintError::UnknownConstraint)
    );

    assert_eq!(simulation.snapshot(), snapshot);
    assert_eq!(committed_train_state(&simulation), trains);
    assert_eq!(ResourceView::derive(simulation.trains()), resources);
    assert_eq!(simulation.next_train_id, next_train_id);
    assert_eq!(simulation.constraints, records);
    assert_eq!(simulation.next_constraint_id, 1);
}

#[test]
fn removing_one_duplicate_leaves_the_other() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    let first = simulation
        .create_constraint_at(closed(a), 1, None, ConstraintOrigin::Planned)
        .unwrap();
    let second = simulation
        .create_constraint_at(closed(a), 1, None, ConstraintOrigin::Planned)
        .unwrap();
    simulation.step();

    simulation.remove_constraint(first).unwrap();

    assert_eq!(simulation.constraints.len(), 1);
    assert_eq!(simulation.constraints[0].id, second);
    assert!(
        !simulation
            .active_constraint_view()
            .permits_departure(a, StationId(1))
    );
}

#[test]
fn active_view_contains_only_constraints_active_at_current_time() {
    let (mut simulation, [a, b, c]) = constraint_simulation();
    simulation
        .create_constraint_at(closed(a), 1, Some(3), ConstraintOrigin::Planned)
        .unwrap();
    simulation
        .create_constraint_at(closed(c), 2, None, ConstraintOrigin::Injected)
        .unwrap();

    // (time, a blocked, c blocked) using a->b and b->c departures.
    for (time, a_blocked, c_blocked) in [
        (0, false, false),
        (1, true, false),
        (2, true, true),
        (3, false, true),
        (4, false, true),
    ] {
        while simulation.elapsed_seconds < time {
            simulation.step();
        }
        let view = simulation.active_constraint_view();
        assert_eq!(!view.permits_departure(a, b), a_blocked, "a at {time}");
        assert_eq!(!view.permits_departure(b, c), c_blocked, "c at {time}");
    }
}

#[test]
fn step_prunes_expired_records_at_new_time_and_keeps_scheduled_ones() {
    let (mut simulation, [a, b, _]) = constraint_simulation();
    let finite = simulation
        .create_constraint_at(closed(a), 1, Some(3), ConstraintOrigin::Planned)
        .unwrap();
    let indefinite = simulation
        .create_constraint_at(closed(a), 1, None, ConstraintOrigin::Planned)
        .unwrap();
    let scheduled = simulation
        .create_constraint_at(closed(b), 9, Some(10), ConstraintOrigin::Planned)
        .unwrap();

    simulation.step();
    simulation.step();
    assert_eq!(simulation.constraints.len(), 3);
    // Observation must not prune.
    simulation.snapshot();
    assert_eq!(simulation.constraints.len(), 3);

    simulation.step();
    let ids: Vec<_> = simulation.constraints.iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![indefinite, scheduled]);
    assert_eq!(
        simulation.remove_constraint(finite),
        Err(ConstraintError::UnknownConstraint)
    );
    assert_eq!(
        simulation.create_constraint_at(closed(a), 10, None, ConstraintOrigin::Planned),
        Ok(ConstraintId(3))
    );
}
