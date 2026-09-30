use std::time::Duration;

use bevy::prelude::*;
use bevy::time::Real;

use crate::scenario::{CoreSimulation, LatestSnapshot};

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

/// The single runtime mutation boundary: tick core, then publish for presentation.
pub(super) fn drive_core(
    time: Res<Time<Real>>,
    mut clock: ResMut<SimulationClock>,
    mut simulation: ResMut<CoreSimulation>,
    mut snapshot: ResMut<LatestSnapshot>,
) {
    // Real time avoids the default virtual clock's long-frame clamp.
    let steps = clock.accumulate(time.delta());
    for _ in 0..steps {
        simulation.0.step();
    }

    // Publish even when no tick is due; elapsed-time equality is not a cache key.
    snapshot.0 = simulation.0.snapshot();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{PlayerTrain, ScenarioStations, initialize_scenario};
    use metro_core::snapshot::TrainSnapshotState;
    use metro_core::train::Direction;

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
        app.insert_resource(Time::<Real>::default());
        register(&mut app);
        app
    }

    fn update(app: &mut App, delta: Duration) {
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(delta);
        app.update();
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
