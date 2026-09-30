use bevy::prelude::*;
use metro_core::snapshot::{SimulationSnapshot, TrainSnapshot, TrainSnapshotState};
use metro_core::station::StationId;
use metro_core::train::TrainId;

use crate::scenario::ScenarioStations;

struct StationPresentation {
    // Matches snapshot stations by identity, independently of layout order.
    id: StationId,
    label: &'static str,
    position: Vec2,
}

/// Drawing metadata only: these positions do not define railway topology or timing.
#[derive(Resource)]
struct StationLayout([StationPresentation; 4]);

impl StationLayout {
    fn position(&self, station: StationId) -> Option<Vec2> {
        self.0
            .iter()
            .find(|entry| entry.id == station)
            .map(|entry| entry.position)
    }

    fn from_scenario(stations: &ScenarioStations) -> Self {
        let positions = [
            Vec2::new(-300.0, 0.0),
            Vec2::new(-100.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(300.0, 0.0),
        ];

        // The fixture retains stations in A/B/C/D order; IDs are copied unchanged.
        Self(std::array::from_fn(|index| StationPresentation {
            id: stations.0[index].id,
            label: stations.0[index].label,
            position: positions[index],
        }))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ProjectionError {
    UnknownStation(StationId),
    ZeroTravelDuration,
    ElapsedNotBeforeArrival,
}

/// Projects one committed observation; does not advance time or infer arrival.
fn project_train_position(
    state: &TrainSnapshotState,
    layout: &StationLayout,
) -> Result<Vec2, ProjectionError> {
    match state {
        TrainSnapshotState::Dwelling { station, .. } | TrainSnapshotState::Ready { station } => {
            layout
                .position(*station)
                .ok_or(ProjectionError::UnknownStation(*station))
        }
        TrainSnapshotState::Moving {
            from,
            to,
            elapsed_seconds,
            travel_seconds,
        } => {
            if *travel_seconds == 0 {
                return Err(ProjectionError::ZeroTravelDuration);
            }
            if elapsed_seconds >= travel_seconds {
                return Err(ProjectionError::ElapsedNotBeforeArrival);
            }
            let from_position = layout
                .position(*from)
                .ok_or(ProjectionError::UnknownStation(*from))?;
            let to_position = layout
                .position(*to)
                .ok_or(ProjectionError::UnknownStation(*to))?;
            let progress = *elapsed_seconds as f64 / *travel_seconds as f64;
            Ok(from_position + (to_position - from_position) * progress as f32)
        }
    }
}

fn find_train_snapshot(snapshot: &SimulationSnapshot, train_id: TrainId) -> Option<&TrainSnapshot> {
    snapshot.trains.iter().find(|train| train.id == train_id)
}

pub(super) fn setup_stations(mut commands: Commands, stations: Res<ScenarioStations>) {
    let layout = StationLayout::from_scenario(&stations);
    let [a, _, _, d] = layout.0.each_ref();
    let midpoint = (a.position + d.position) / 2.0;

    commands.spawn((
        Sprite::from_color(
            Color::srgb(0.6, 0.6, 0.6),
            Vec2::new(d.position.x - a.position.x, 3.0),
        ),
        Transform::from_translation(midpoint.extend(-2.0)),
    ));

    for station in &layout.0 {
        commands.spawn((
            Sprite::from_color(Color::srgb(0.0, 1.0, 1.0), Vec2::splat(14.0)),
            Transform::from_translation(station.position.extend(2.0)),
        ));
        commands.spawn((
            Text2d::new(station.label),
            TextFont::from_font_size(24.0),
            TextColor(Color::WHITE),
            Transform::from_translation((station.position + Vec2::new(0.0, -32.0)).extend(3.0)),
        ));
    }

    commands.insert_resource(layout);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::ScenarioStation;
    use metro_core::train::Direction;

    fn test_layout() -> StationLayout {
        StationLayout::from_scenario(&ScenarioStations(
            [(42, "A"), (7, "B"), (101, "C"), (23, "D")].map(|(id, label)| ScenarioStation {
                id: StationId(id),
                label,
            }),
        ))
    }

    fn moving(
        from: usize,
        to: usize,
        elapsed_seconds: u64,
        travel_seconds: u64,
    ) -> TrainSnapshotState {
        TrainSnapshotState::Moving {
            from: StationId(from),
            to: StationId(to),
            elapsed_seconds,
            travel_seconds,
        }
    }

    fn assert_position_close(actual: Vec2, expected: Vec2) {
        assert!(actual.is_finite());
        assert!(
            (actual.x - expected.x).abs() < 1e-4,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.y - expected.y).abs() < 1e-4,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn station_lookup_matches_ids_not_indices() {
        let layout = test_layout();
        assert_eq!(layout.position(StationId(42)), Some(Vec2::new(-300.0, 0.0)));
        assert_eq!(layout.position(StationId(7)), Some(Vec2::new(-100.0, 0.0)));
        assert_eq!(layout.position(StationId(101)), Some(Vec2::new(100.0, 0.0)));
        assert_eq!(layout.position(StationId(23)), Some(Vec2::new(300.0, 0.0)));
        assert_eq!(layout.position(StationId(0)), None);
    }

    #[test]
    fn dwelling_and_ready_project_exactly_to_each_station() {
        let layout = test_layout();
        for entry in &layout.0 {
            for remaining_seconds in [0, 1, 3, 99] {
                let state = TrainSnapshotState::Dwelling {
                    station: entry.id,
                    remaining_seconds,
                };
                assert_eq!(project_train_position(&state, &layout), Ok(entry.position));
            }
            let state = TrainSnapshotState::Ready { station: entry.id };
            assert_eq!(project_train_position(&state, &layout), Ok(entry.position));
        }
    }

    #[test]
    fn moving_projects_committed_forward_progress() {
        let layout = test_layout();
        let departure = project_train_position(&moving(42, 7, 0, 3), &layout).unwrap();
        assert!(departure.is_finite());
        assert_eq!(departure, Vec2::new(-300.0, 0.0));
        for (elapsed, x) in [(1, -233.333333), (2, -166.666667)] {
            assert_position_close(
                project_train_position(&moving(42, 7, elapsed, 3), &layout).unwrap(),
                Vec2::new(x, 0.0),
            );
        }
    }

    #[test]
    fn moving_projects_reverse_endpoints_as_supplied() {
        let layout = test_layout();
        assert_position_close(
            project_train_position(&moving(7, 42, 1, 3), &layout).unwrap(),
            Vec2::new(-166.666667, 0.0),
        );

        // Y precedes X in storage; neither ID ordering nor facing selects endpoints.
        let mut unequal = test_layout();
        unequal.0[1].position = Vec2::new(-20.0, -8.0);
        unequal.0[2].position = Vec2::new(140.0, 12.0);
        assert_position_close(
            project_train_position(&moving(101, 7, 1, 4), &unequal).unwrap(),
            Vec2::new(100.0, 7.0),
        );
    }

    #[test]
    fn layout_distance_changes_only_projected_coordinates() {
        let short = test_layout();
        let mut long = test_layout();
        long.0[1].position = Vec2::new(300.0, 0.0);
        let state = moving(42, 7, 1, 3);
        let retained = state.clone();
        assert_position_close(
            project_train_position(&state, &short).unwrap(),
            Vec2::new(-233.333333, 0.0),
        );
        assert_position_close(
            project_train_position(&state, &long).unwrap(),
            Vec2::new(-100.0, 0.0),
        );
        assert_eq!(state, retained);
    }

    #[test]
    fn facing_does_not_affect_position() {
        let layout = test_layout();
        let forward = TrainSnapshot {
            id: TrainId(42),
            direction: Direction::Forward,
            state: moving(7, 42, 1, 3),
            velocity: 1,
        };
        let backward = TrainSnapshot {
            direction: Direction::Backward,
            ..forward.clone()
        };
        let position = project_train_position(&forward.state, &layout).unwrap();
        assert_position_close(position, Vec2::new(-166.666667, 0.0));
        assert_eq!(
            project_train_position(&backward.state, &layout),
            Ok(position)
        );
    }

    #[test]
    fn invalid_moving_times_return_errors() {
        let layout = test_layout();
        for (elapsed, travel, error) in [
            (0, 0, ProjectionError::ZeroTravelDuration),
            (3, 3, ProjectionError::ElapsedNotBeforeArrival),
            (4, 3, ProjectionError::ElapsedNotBeforeArrival),
        ] {
            assert_eq!(
                project_train_position(&moving(42, 7, elapsed, travel), &layout),
                Err(error)
            );
        }
    }

    #[test]
    fn missing_station_in_each_state_or_endpoint_returns_error() {
        let layout = test_layout();
        let missing = StationId(999);
        for state in [
            TrainSnapshotState::Dwelling {
                station: missing,
                remaining_seconds: 3,
            },
            TrainSnapshotState::Ready { station: missing },
            moving(999, 7, 1, 3),
            moving(42, 999, 1, 3),
        ] {
            assert_eq!(
                project_train_position(&state, &layout),
                Err(ProjectionError::UnknownStation(missing))
            );
        }
    }

    #[test]
    fn train_lookup_matches_id_independently_of_vector_order() {
        let target = TrainSnapshot {
            id: TrainId(42),
            direction: Direction::Backward,
            state: moving(7, 42, 1, 3),
            velocity: 1,
        };
        let other = |id| TrainSnapshot {
            id: TrainId(id),
            direction: Direction::Forward,
            state: TrainSnapshotState::Ready {
                station: StationId(101),
            },
            velocity: 0,
        };
        let mut snapshot = SimulationSnapshot {
            elapsed_seconds: 5,
            trains: vec![other(9), target.clone(), other(2)],
        };
        assert_eq!(find_train_snapshot(&snapshot, target.id), Some(&target));
        snapshot.trains.rotate_left(2);
        assert_eq!(find_train_snapshot(&snapshot, target.id), Some(&target));
        assert_eq!(find_train_snapshot(&snapshot, TrainId(999)), None);
        snapshot.trains.clear();
        assert_eq!(find_train_snapshot(&snapshot, target.id), None);
    }

    #[test]
    fn layout_preserves_station_metadata_and_fixed_positions() {
        let stations = ScenarioStations([(42, "A"), (7, "B"), (101, "C"), (23, "D")].map(
            |(id, label)| ScenarioStation {
                id: StationId(id),
                label,
            },
        ));

        let layout = StationLayout::from_scenario(&stations);

        assert_eq!(
            layout
                .0
                .each_ref()
                .map(|station| (station.id, station.label, station.position)),
            [
                (StationId(42), "A", Vec2::new(-300.0, 0.0)),
                (StationId(7), "B", Vec2::new(-100.0, 0.0)),
                (StationId(101), "C", Vec2::new(100.0, 0.0)),
                (StationId(23), "D", Vec2::new(300.0, 0.0)),
            ]
        );
    }
}
