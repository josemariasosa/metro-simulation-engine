use super::*;

#[test]
fn new_simulation_has_empty_constraint_registry() {
    let (simulation, _) = constraint_simulation();

    assert!(simulation.constraints.is_empty());
    assert_eq!(simulation.next_constraint_id, 0);
}

#[test]
fn registration_stores_exact_canonical_record_for_both_origins() {
    let (mut simulation, [a, b, _]) = constraint_simulation();
    let track = OperationalConstraint::TrackUnavailable { from: a, to: b };

    let planned = simulation
        .create_constraint_at(track, 1, Some(5), ConstraintOrigin::Planned)
        .unwrap();
    let injected = simulation
        .create_constraint_at(closed(b), 7, None, ConstraintOrigin::Injected)
        .unwrap();

    assert_eq!(
        simulation.constraints,
        vec![
            ConstraintRecord {
                id: planned,
                constraint: track,
                start_at: 1,
                end_at: Some(5),
                origin: ConstraintOrigin::Planned,
            },
            ConstraintRecord {
                id: injected,
                constraint: closed(b),
                start_at: 7,
                end_at: None,
                origin: ConstraintOrigin::Injected,
            },
        ]
    );
}

#[test]
fn ids_are_monotonic_distinct_and_not_consumed_by_failures() {
    let (mut simulation, [a, ..]) = constraint_simulation();

    let first = simulation
        .create_constraint_at(closed(a), 5, None, ConstraintOrigin::Planned)
        .unwrap();
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidStart, |s| {
        s.create_constraint_at(closed(a), 0, None, ConstraintOrigin::Planned)
    });
    let duplicate = simulation
        .create_constraint_at(closed(a), 5, None, ConstraintOrigin::Planned)
        .unwrap();

    assert_eq!((first, duplicate), (ConstraintId(0), ConstraintId(1)));
    assert_eq!(simulation.constraints.len(), 2);
    assert_eq!(
        simulation.constraints[0].constraint,
        simulation.constraints[1].constraint
    );
}

#[test]
fn id_exhaustion_fails_before_mutation_and_last_id_is_never_issued() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    simulation.next_constraint_id = u64::MAX - 1;

    let last = simulation
        .create_constraint_at(closed(a), 5, None, ConstraintOrigin::Planned)
        .unwrap();
    assert_eq!(last, ConstraintId(u64::MAX - 1));

    assert_constraint_failure_atomic(&mut simulation, ConstraintError::IdExhausted, |s| {
        s.create_constraint_at(closed(a), 5, None, ConstraintOrigin::Planned)
    });
}
