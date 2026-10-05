use std::time::Duration;

use bevy::prelude::*;
use bevy::time::Real;
use metro_core::command::TrainCommand;

use crate::scenario::{CoreSimulation, LatestSnapshot, PlayerTrain};

#[derive(Resource, Default)]
pub(super) struct SimulationClock {
    pending: Duration,
}

impl SimulationClock {
    /// Consume every whole second while preserving the exact fractional remainder.
    fn accumulate(&mut self, frame_delta: Duration) -> u64 {
        self.pending += frame_delta;
        let steps = self.pending.as_secs();
        self.pending -= Duration::from_secs(steps);
        steps
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<SimulationClock>()
        .add_systems(Update, drive_core);
}

/// The single runtime mutation boundary: tick, apply input, then publish.
pub(super) fn drive_core(
    time: Res<Time<Real>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    player: Res<PlayerTrain>,
    mut clock: ResMut<SimulationClock>,
    mut simulation: ResMut<CoreSimulation>,
    mut snapshot: ResMut<LatestSnapshot>,
) {
    // Real time avoids the default virtual clock's long-frame clamp.
    let steps = clock.accumulate(time.delta());
    for _ in 0..steps {
        simulation.0.step();
    }

    // Commands observe every due tick, including any arrival committed this frame.
    if keyboard.just_pressed(KeyCode::Space) {
        let train_id = player.0;
        if let Err(error) = simulation
            .0
            .apply_command(TrainCommand::Accelerate { train_id })
        {
            warn!("Accelerate for train {:?}: {:?}", train_id, error);
        }
    }

    // Publish even when no tick is due; elapsed-time equality is not a cache key.
    snapshot.0 = simulation.0.snapshot();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{PlayerTrain, ScenarioStations, initialize_scenario};
    use metro_core::snapshot::{TrainSnapshot, TrainSnapshotState};
    use metro_core::{Direction, TrainId};

    #[test]
    fn accumulator_consumes_whole_seconds_and_retains_remainder() {
        let mut clock = SimulationClock::default();
        assert_eq!(clock.pending, Duration::ZERO);
        for (milliseconds, steps, remainder) in [(400, 0, 400), (600, 1, 0), (2500, 2, 500)] {
            assert_eq!(clock.accumulate(Duration::from_millis(milliseconds)), steps);
            assert_eq!(clock.pending, Duration::from_millis(remainder));
        }
        let mut clock = SimulationClock::default();
        assert_eq!(clock.accumulate(Duration::from_millis(2500)), 2);
        assert_eq!(clock.pending, Duration::from_millis(500));
    }

    #[test]
    fn equivalent_frame_partitions_produce_equal_steps_and_remainders() {
        fn accumulate_frames(frames: &[u64]) -> (u64, Duration) {
            let mut clock = SimulationClock::default();
            let steps = frames
                .iter()
                .map(|&milliseconds| clock.accumulate(Duration::from_millis(milliseconds)))
                .sum();
            (steps, clock.pending)
        }

        for (whole, partition, expected) in [
            (1000, vec![400, 600], (1, Duration::ZERO)),
            (3500, vec![400, 600, 2500], (3, Duration::from_millis(500))),
        ] {
            assert_eq!(accumulate_frames(&[whole]), expected);
            assert_eq!(accumulate_frames(&partition), expected);
        }
    }

    #[test]
    fn zero_delta_preserves_pending_duration() {
        let mut clock = SimulationClock::default();
        assert_eq!(clock.accumulate(Duration::ZERO), 0);
        assert_eq!(clock.pending, Duration::ZERO);
        assert_eq!(clock.accumulate(Duration::from_millis(400)), 0);
        assert_eq!(clock.accumulate(Duration::ZERO), 0);
        assert_eq!(clock.pending, Duration::from_millis(400));
    }

    #[test]
    fn accumulator_preserves_nanosecond_precision() {
        let mut clock = SimulationClock::default();
        assert_eq!(clock.accumulate(Duration::from_nanos(999_999_999)), 0);
        assert_eq!(clock.pending, Duration::from_nanos(999_999_999));
        assert_eq!(clock.accumulate(Duration::from_nanos(1)), 1);
        assert_eq!(clock.pending, Duration::ZERO);
    }

    fn driver_app() -> App {
        let mut app = App::new();
        initialize_scenario(&mut app);
        app.insert_resource(Time::<Real>::default())
            .init_resource::<ButtonInput<KeyCode>>();
        register(&mut app);
        app
    }

    fn update(app: &mut App, delta: Duration) {
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(delta);
        app.update();
        // Stand in for InputPlugin's frame-edge clearing, preserving held keys.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }

    fn press_space(app: &mut App) {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.release(KeyCode::Space);
        keyboard.press(KeyCode::Space);
    }

    fn published_player(app: &App) -> &TrainSnapshot {
        let world = app.world();
        let snapshot = &world.resource::<LatestSnapshot>().0;
        assert_eq!(*snapshot, world.resource::<CoreSimulation>().0.snapshot());
        let player = world.resource::<PlayerTrain>().0;
        snapshot
            .trains
            .iter()
            .find(|train| train.id == player)
            .unwrap()
    }

    fn assert_moving(app: &App, from: usize, to: usize, elapsed_seconds: u64) {
        let stations = &app.world().resource::<ScenarioStations>().0;
        let train = published_player(app);
        assert_eq!(train.velocity, 1);
        assert_eq!(train.direction, Direction::Forward);
        assert_eq!(
            train.state,
            TrainSnapshotState::Moving {
                from: stations[from].id,
                to: stations[to].id,
                elapsed_seconds,
                travel_seconds: 3,
            }
        );
    }

    #[test]
    fn driver_publishes_command_without_due_tick() {
        let mut app = driver_app();
        press_space(&mut app);
        update(&mut app, Duration::from_millis(400));
        assert_moving(&app, 0, 1, 0);
        assert_eq!(
            app.world().resource::<LatestSnapshot>().0.elapsed_seconds,
            0
        );
        assert_eq!(
            app.world().resource::<SimulationClock>().pending,
            Duration::from_millis(400)
        );
    }

    #[test]
    fn driver_applies_command_after_arrival_and_all_catch_up_ticks() {
        for (delta_ms, elapsed, pending_ms) in [(1000, 3, 0), (2500, 4, 500)] {
            let mut app = driver_app();
            press_space(&mut app);
            update(&mut app, Duration::ZERO);
            update(&mut app, Duration::from_secs(2));
            assert_moving(&app, 0, 1, 2);

            press_space(&mut app);
            update(&mut app, Duration::from_millis(delta_ms));
            assert_moving(&app, 1, 2, 0);
            assert_eq!(
                app.world().resource::<LatestSnapshot>().0.elapsed_seconds,
                elapsed
            );
            assert_eq!(
                app.world().resource::<SimulationClock>().pending,
                Duration::from_millis(pending_ms)
            );
        }
    }

    #[test]
    fn driver_moving_presses_preserve_progress_and_velocity() {
        let mut app = driver_app();
        press_space(&mut app);
        update(&mut app, Duration::ZERO);
        update(&mut app, Duration::from_secs(1));
        assert_moving(&app, 0, 1, 1);
        let before = app.world().resource::<CoreSimulation>().0.snapshot();
        for _ in 0..3 {
            press_space(&mut app);
            update(&mut app, Duration::ZERO);
            assert_moving(&app, 0, 1, 1);
            assert_eq!(app.world().resource::<LatestSnapshot>().0, before);
        }
    }

    #[test]
    fn driver_rejected_command_still_publishes_and_is_not_retried() {
        let mut app = driver_app();
        let player = app.world().resource::<PlayerTrain>().0;
        let before = app.world().resource::<CoreSimulation>().0.snapshot();
        app.world_mut().resource_mut::<PlayerTrain>().0 = TrainId(999);
        app.world_mut()
            .resource_mut::<LatestSnapshot>()
            .0
            .trains
            .clear();
        press_space(&mut app);
        update(&mut app, Duration::from_millis(400));
        assert_eq!(app.world().resource::<LatestSnapshot>().0, before);
        assert_eq!(
            app.world().resource::<CoreSimulation>().0.snapshot(),
            before
        );
        assert_eq!(
            app.world().resource::<SimulationClock>().pending,
            Duration::from_millis(400)
        );

        app.world_mut().resource_mut::<PlayerTrain>().0 = player;
        update(&mut app, Duration::ZERO);
        assert_eq!(app.world().resource::<LatestSnapshot>().0, before);
    }

    #[test]
    fn driver_held_space_does_not_depart_after_arrival_or_dwell() {
        let mut app = driver_app();
        press_space(&mut app);
        update(&mut app, Duration::ZERO);
        assert_moving(&app, 0, 1, 0);
        let b = app.world().resource::<ScenarioStations>().0[1].id;

        for tick in 1..=7 {
            {
                let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                assert!(keyboard.pressed(KeyCode::Space));
                keyboard.press(KeyCode::Space);
                assert!(!keyboard.just_pressed(KeyCode::Space));
            }
            update(&mut app, Duration::from_secs(1));
            if tick < 3 {
                assert_moving(&app, 0, 1, tick);
            } else {
                let train = published_player(&app);
                assert_eq!(train.velocity, 0);
                assert_eq!(
                    train.state,
                    if tick < 6 {
                        TrainSnapshotState::Dwelling {
                            station: b,
                            remaining_seconds: 6 - tick,
                        }
                    } else {
                        TrainSnapshotState::Ready { station: b }
                    }
                );
            }
        }
        press_space(&mut app);
        update(&mut app, Duration::ZERO);
        assert_moving(&app, 1, 2, 0);
    }

    fn assert_published_state(app: &App, elapsed: u64, remaining: Option<u64>, pending: Duration) {
        let world = app.world();
        let snapshot = &world.resource::<LatestSnapshot>().0;
        assert_eq!(*snapshot, world.resource::<CoreSimulation>().0.snapshot());
        assert_eq!(snapshot.elapsed_seconds, elapsed);
        assert_eq!(world.resource::<SimulationClock>().pending, pending);
        let player = world.resource::<PlayerTrain>().0;
        let station = world.resource::<ScenarioStations>().0[0].id;
        let train = snapshot
            .trains
            .iter()
            .find(|train| train.id == player)
            .unwrap();
        assert_eq!(train.direction, Direction::Forward);
        assert_eq!(train.velocity, 0);
        assert_eq!(
            train.state,
            match remaining {
                Some(remaining_seconds) => TrainSnapshotState::Dwelling {
                    station,
                    remaining_seconds
                },
                None => TrainSnapshotState::Ready { station },
            }
        );
    }

    #[test]
    fn driver_paces_and_publishes_manual_scenario_without_departure() {
        let mut app = driver_app();
        assert_published_state(&app, 0, Some(3), Duration::ZERO);
        for (delta, elapsed, remaining, pending) in [
            (400, 0, Some(3), 400),
            (600, 1, Some(2), 0),
            (1000, 2, Some(1), 0),
            (1000, 3, None, 0),
            (10_500, 13, None, 500),
        ] {
            update(&mut app, Duration::from_millis(delta));
            assert_published_state(&app, elapsed, remaining, Duration::from_millis(pending));
        }
    }

    #[test]
    fn driver_republishes_snapshot_on_updates_without_steps() {
        let mut app = driver_app();
        update(&mut app, Duration::ZERO);
        for delta in [Duration::ZERO, Duration::from_millis(400)] {
            // Keep the timestamp equal to core while making the cache observably stale.
            app.world_mut()
                .resource_mut::<LatestSnapshot>()
                .0
                .trains
                .clear();
            update(&mut app, delta);
            assert_published_state(&app, 0, Some(3), delta);
        }
    }
}
