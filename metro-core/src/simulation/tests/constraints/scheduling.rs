use super::*;

#[test]
fn relative_and_absolute_scheduling_create_equivalent_records() {
    for origin in [ConstraintOrigin::Planned, ConstraintOrigin::Injected] {
        let (mut absolute, [a, ..]) = constraint_simulation();
        let (mut relative, _) = constraint_simulation();
        absolute.elapsed_seconds = 10;
        relative.elapsed_seconds = 10;

        let absolute_id = absolute
            .create_constraint_at(closed(a), 11, Some(15), origin)
            .unwrap();
        let relative_id = relative
            .create_constraint_in(closed(a), 1, Some(4), origin)
            .unwrap();

        assert_eq!(absolute_id, relative_id);
        assert_eq!(absolute.constraints, relative.constraints);
        assert_eq!(absolute.next_constraint_id, relative.next_constraint_id);
    }
}

#[test]
fn relative_zero_delay_is_invalid_and_wins_over_zero_duration() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    for duration in [None, Some(0), Some(4)] {
        assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidStart, |s| {
            s.create_constraint_in(closed(a), 0, duration, ConstraintOrigin::Planned)
        });
    }
}

#[test]
fn relative_zero_duration_is_invalid_and_wins_over_start_overflow() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    for now in [10, u64::MAX] {
        simulation.elapsed_seconds = now;
        assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidEnd, |s| {
            s.create_constraint_in(closed(a), 1, Some(0), ConstraintOrigin::Planned)
        });
    }
}

#[test]
fn relative_start_overflow_is_atomic() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    simulation
        .create_constraint_at(closed(a), 1, None, ConstraintOrigin::Planned)
        .unwrap();
    simulation.elapsed_seconds = u64::MAX;
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::TimeOverflow, |s| {
        s.create_constraint_in(closed(a), 1, None, ConstraintOrigin::Injected)
    });
}

#[test]
fn relative_end_overflow_is_atomic() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    simulation
        .create_constraint_at(closed(a), 1, None, ConstraintOrigin::Planned)
        .unwrap();
    simulation.elapsed_seconds = u64::MAX - 2;
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::TimeOverflow, |s| {
        s.create_constraint_in(closed(a), 1, Some(2), ConstraintOrigin::Injected)
    });
}

#[test]
fn relative_duration_is_measured_from_activation() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    simulation.elapsed_seconds = 10;
    let id = simulation
        .create_constraint_in(closed(a), 3, Some(4), ConstraintOrigin::Injected)
        .unwrap();
    assert_eq!(
        simulation.constraints,
        vec![ConstraintRecord {
            id,
            constraint: closed(a),
            start_at: 13,
            end_at: Some(17),
            origin: ConstraintOrigin::Injected,
        }]
    );
}

#[test]
fn relative_indefinite_constraint_has_no_end() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    simulation.elapsed_seconds = 10;
    simulation
        .create_constraint_in(closed(a), 2, None, ConstraintOrigin::Planned)
        .unwrap();
    assert_eq!(simulation.constraints[0].start_at, 12);
    assert_eq!(simulation.constraints[0].end_at, None);
}

#[test]
fn relative_normalization_precedes_canonical_validation() {
    let (mut simulation, _) = constraint_simulation();
    simulation.next_constraint_id = u64::MAX;
    let unknown = closed(StationId(99));
    for (now, delay, duration, error) in [
        (10, 0, Some(0), ConstraintError::InvalidStart),
        (u64::MAX, 1, Some(0), ConstraintError::InvalidEnd),
        (u64::MAX, 1, None, ConstraintError::TimeOverflow),
        (u64::MAX - 2, 1, Some(2), ConstraintError::TimeOverflow),
    ] {
        simulation.elapsed_seconds = now;
        assert_constraint_failure_atomic(&mut simulation, error, |s| {
            s.create_constraint_in(unknown, delay, duration, ConstraintOrigin::Planned)
        });
    }
}

#[test]
fn relative_scheduling_delegates_canonical_validation() {
    let (mut simulation, [a, _, c]) = constraint_simulation();
    simulation.next_constraint_id = u64::MAX;
    for (target, error) in [
        (closed(StationId(99)), ConstraintError::UnknownStation),
        (
            OperationalConstraint::TrackUnavailable { from: a, to: c },
            ConstraintError::UnknownTrack,
        ),
        (closed(a), ConstraintError::IdExhausted),
    ] {
        assert_constraint_failure_atomic(&mut simulation, error, |s| {
            s.create_constraint_in(target, 1, None, ConstraintOrigin::Planned)
        });
    }
}

#[test]
fn relative_and_absolute_scheduling_share_registry_and_allocator() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    let first = simulation
        .create_constraint_at(closed(a), 1, Some(5), ConstraintOrigin::Planned)
        .unwrap();
    let second = simulation
        .create_constraint_in(closed(a), 1, Some(4), ConstraintOrigin::Planned)
        .unwrap();
    let third = simulation
        .create_constraint_at(closed(a), 1, Some(5), ConstraintOrigin::Planned)
        .unwrap();
    assert_eq!(
        (first, second, third),
        (ConstraintId(0), ConstraintId(1), ConstraintId(2))
    );
    assert_eq!(simulation.next_constraint_id, 3);
    assert_eq!(simulation.constraints.len(), 3);
    for (record, id) in simulation.constraints.iter().zip([first, second, third]) {
        assert_eq!(
            record,
            &ConstraintRecord {
                id,
                constraint: closed(a),
                start_at: 1,
                end_at: Some(5),
                origin: ConstraintOrigin::Planned,
            }
        );
    }
}

#[test]
fn absolute_start_must_be_strictly_after_current_time() {
    let (mut simulation, [a, ..]) = constraint_simulation();

    assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidStart, |s| {
        s.create_constraint_at(closed(a), 0, None, ConstraintOrigin::Planned)
    });
    assert!(
        simulation
            .create_constraint_at(closed(a), 1, None, ConstraintOrigin::Planned)
            .is_ok()
    );

    simulation.step();
    simulation.step();
    for start in [1, 2] {
        assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidStart, |s| {
            s.create_constraint_at(closed(a), start, None, ConstraintOrigin::Planned)
        });
    }
    assert!(
        simulation
            .create_constraint_at(closed(a), 3, None, ConstraintOrigin::Planned)
            .is_ok()
    );
}

#[test]
fn finite_end_must_be_strictly_after_start() {
    let (mut simulation, [a, ..]) = constraint_simulation();

    for end in [5, 4, 0] {
        assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidEnd, |s| {
            s.create_constraint_at(closed(a), 5, Some(end), ConstraintOrigin::Planned)
        });
    }
    assert!(
        simulation
            .create_constraint_at(closed(a), 5, Some(6), ConstraintOrigin::Planned)
            .is_ok()
    );
}
