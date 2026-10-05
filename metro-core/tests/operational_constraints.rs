use metro_core::{
    ConstraintError, ConstraintId, ConstraintOrigin, Direction, DwellPolicy, Network,
    OperationalConstraint, Train, TrainId,
    command::{CommandError, TrainCommand},
    simulation::Simulation,
    snapshot::TrainSnapshotState,
};

#[test]
fn public_scheduling_apis_have_operational_effect() {
    for relative in [false, true] {
        for manual in [false, true] {
            let mut network = Network::new();
            let a = network.add_station("A");
            let b = network.add_station("B");
            network.connect_bidirectional(a, b, 3);
            let constructor = if manual {
                Train::new_manual
            } else {
                Train::new
            };
            let mut sim = Simulation::new(
                network,
                vec![constructor(100, a, Direction::Forward, 2)],
                DwellPolicy::new(),
            );
            let value = OperationalConstraint::TrackUnavailable { from: a, to: b };
            let id: ConstraintId = if relative {
                sim.create_constraint_in(value, 1, None, ConstraintOrigin::Injected)
            } else {
                sim.create_constraint_at(value, 1, None, ConstraintOrigin::Planned)
            }
            .unwrap();
            sim.step();
            let accelerate = TrainCommand::Accelerate {
                train_id: TrainId(0),
            };
            if manual {
                let before = sim.snapshot();
                assert_eq!(sim.apply_command(accelerate), Err(CommandError::Blocked));
                assert_eq!(sim.snapshot(), before);
            }
            sim.step();
            assert_eq!(
                sim.snapshot().trains[0].state,
                TrainSnapshotState::Ready { station: a }
            );
            sim.remove_constraint(id).unwrap();
            assert_eq!(
                sim.remove_constraint(id),
                Err(ConstraintError::UnknownConstraint)
            );
            assert_eq!(
                sim.snapshot().trains[0].state,
                TrainSnapshotState::Ready { station: a }
            );
            if manual {
                sim.apply_command(TrainCommand::Accelerate {
                    train_id: TrainId(0),
                })
                .unwrap();
            } else {
                sim.step();
            }
            assert_eq!(
                sim.snapshot().trains[0].state,
                TrainSnapshotState::Moving {
                    from: a,
                    to: b,
                    elapsed_seconds: 0,
                    travel_seconds: 3
                }
            );
        }
    }
}

#[test]
fn public_expiry_trace_requires_a_later_departure_attempt() {
    for manual in [false, true] {
        let mut network = Network::new();
        let a = network.add_station("A");
        let b = network.add_station("B");
        network.connect_bidirectional(a, b, 3);
        let constructor = if manual {
            Train::new_manual
        } else {
            Train::new
        };
        let mut sim = Simulation::new(
            network,
            vec![constructor(100, a, Direction::Forward, 12)],
            DwellPolicy::new(),
        );
        for _ in 0..10 {
            sim.step();
        }
        sim.create_constraint_at(
            OperationalConstraint::StationDeparturesBlocked { station: a },
            11,
            Some(15),
            ConstraintOrigin::Planned,
        )
        .unwrap();
        sim.step();
        for time in 11..15 {
            assert_eq!(sim.elapsed_seconds, time);
            if manual {
                assert_eq!(
                    sim.apply_command(TrainCommand::Accelerate {
                        train_id: TrainId(0)
                    }),
                    Err(CommandError::Blocked)
                );
            }
            sim.step();
            assert_eq!(
                sim.snapshot().trains[0].state,
                TrainSnapshotState::Ready { station: a }
            );
        }
        assert_eq!(sim.elapsed_seconds, 15);
        if manual {
            sim.step();
            assert_eq!(
                sim.snapshot().trains[0].state,
                TrainSnapshotState::Ready { station: a }
            );
            sim.apply_command(TrainCommand::Accelerate {
                train_id: TrainId(0),
            })
            .unwrap();
        } else {
            sim.step();
        }
        assert!(matches!(
            sim.snapshot().trains[0].state,
            TrainSnapshotState::Moving {
                elapsed_seconds: 0,
                ..
            }
        ));
    }
}
