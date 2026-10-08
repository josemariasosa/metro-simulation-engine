use super::*;

#[test]
fn validation_follows_spec_precedence() {
    let (mut simulation, [a, b, _]) = constraint_simulation();
    let unknown = StationId(99);
    // An exhausted allocator must not mask any earlier validation error.
    simulation.next_constraint_id = u64::MAX;
    let missing_track = OperationalConstraint::TrackUnavailable {
        from: a,
        to: unknown,
    };

    assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidStart, |s| {
        s.create_constraint_at(missing_track, 0, Some(0), ConstraintOrigin::Planned)
    });
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::InvalidEnd, |s| {
        s.create_constraint_at(missing_track, 5, Some(5), ConstraintOrigin::Planned)
    });
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::UnknownStation, |s| {
        s.create_constraint_at(missing_track, 5, None, ConstraintOrigin::Planned)
    });
    let no_edge = OperationalConstraint::TrackUnavailable {
        from: a,
        to: StationId(2),
    };
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::UnknownTrack, |s| {
        s.create_constraint_at(no_edge, 5, None, ConstraintOrigin::Planned)
    });
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::IdExhausted, |s| {
        s.create_constraint_at(closed(b), 5, None, ConstraintOrigin::Planned)
    });
}

#[test]
fn unknown_station_targets_are_rejected_without_mutation() {
    let (mut simulation, [a, ..]) = constraint_simulation();
    let unknown = StationId(3);
    let targets = [
        OperationalConstraint::StationDeparturesBlocked { station: unknown },
        OperationalConstraint::StationUnavailable { station: unknown },
        OperationalConstraint::TrackUnavailable {
            from: unknown,
            to: a,
        },
        OperationalConstraint::TrackUnavailable {
            from: a,
            to: unknown,
        },
        OperationalConstraint::TrackUnavailable {
            from: unknown,
            to: StationId(4),
        },
    ];

    for target in targets {
        assert_constraint_failure_atomic(&mut simulation, ConstraintError::UnknownStation, |s| {
            s.create_constraint_at(target, 5, None, ConstraintOrigin::Planned)
        });
    }
}

#[test]
fn track_target_requires_exact_directed_edge() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    network.add_track(b, a, 2);
    let mut simulation = Simulation::new(network, vec![], DwellPolicy::new());

    assert_constraint_failure_atomic(&mut simulation, ConstraintError::UnknownTrack, |s| {
        let forward = OperationalConstraint::TrackUnavailable { from: a, to: b };
        s.create_constraint_at(forward, 5, None, ConstraintOrigin::Planned)
    });
    assert_constraint_failure_atomic(&mut simulation, ConstraintError::UnknownTrack, |s| {
        let same = OperationalConstraint::TrackUnavailable { from: a, to: a };
        s.create_constraint_at(same, 5, None, ConstraintOrigin::Planned)
    });
    let reverse = OperationalConstraint::TrackUnavailable { from: b, to: a };
    assert!(
        simulation
            .create_constraint_at(reverse, 5, None, ConstraintOrigin::Planned)
            .is_ok()
    );
}
