use bevy::prelude::*;
use metro_core::snapshot::{SimulationSnapshot, TrainSnapshot, TrainSnapshotState};
use metro_core::station::StationId;
use metro_core::train::{Direction, TrainId};

use crate::scenario::{LatestSnapshot, PlayerTrain, ScenarioStations};

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
    fn label(&self, station: StationId) -> Option<&'static str> {
        self.0
            .iter()
            .find(|entry| entry.id == station)
            .map(|entry| entry.label)
    }

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

#[derive(Component)]
struct TrainEntity {
    train_id: TrainId,
}

#[derive(Component)]
struct TrainBody;

#[derive(Component)]
struct TrainArtwork;

#[derive(Component)]
struct TrainStatus;

#[derive(Debug, PartialEq, Eq)]
enum PresentationError {
    MissingTrainSnapshot(TrainId),
    Projection(ProjectionError),
}

#[derive(Component, Default)]
struct PresentationDiagnostic(Option<PresentationError>);

fn format_train_status(
    state: &TrainSnapshotState,
    layout: &StationLayout,
) -> Result<String, ProjectionError> {
    let label = |station| {
        layout
            .label(station)
            .ok_or(ProjectionError::UnknownStation(station))
    };
    Ok(match state {
        TrainSnapshotState::Dwelling {
            station,
            remaining_seconds,
        } => {
            format!("Dwelling at {} — {remaining_seconds}s", label(*station)?)
        }
        TrainSnapshotState::Ready { station } => format!("Ready at {}", label(*station)?),
        TrainSnapshotState::Moving {
            from,
            to,
            elapsed_seconds,
            travel_seconds,
        } => {
            format!(
                "Moving {} → {} — {elapsed_seconds}/{travel_seconds}",
                label(*from)?,
                label(*to)?
            )
        }
    })
}

// Keep system parameter types private to presentation.
pub(super) fn register(app: &mut App) {
    app.add_systems(Startup, (setup_stations, setup_train))
        .add_systems(
            Update,
            (present_trains, bounce_train).after(crate::timing::drive_core),
        );
}

fn setup_train(mut commands: Commands, assets: Res<AssetServer>, player: Res<PlayerTrain>) {
    commands
        .spawn((
            TrainEntity { train_id: player.0 },
            PresentationDiagnostic::default(),
            Sprite::from_color(Color::srgb(1.0, 0.5, 0.0), Vec2::new(32.0, 12.0)),
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                TrainArtwork,
                Sprite::from_image(assets.load("trains/train_base.png")),
                Transform::from_xyz(0.0, 0.0, -4.0).with_scale(Vec3::splat(0.25)),
            ));
            parent.spawn((
                TrainArtwork,
                TrainBody,
                Sprite::from_image(assets.load("trains/train_body.png")),
                Transform::from_xyz(0.0, 0.0, -3.0).with_scale(Vec3::splat(0.25)),
            ));
            parent.spawn((
                TrainStatus,
                Text2d::new(""),
                TextFont::from_font_size(24.0),
                TextColor(Color::WHITE),
                Transform::from_xyz(0.0, 70.0, 1.0),
            ));
        });
}

fn present_trains(
    snapshot: Res<LatestSnapshot>,
    layout: Res<StationLayout>,
    mut trains: Query<(
        &TrainEntity,
        &mut Transform,
        &mut Visibility,
        &mut PresentationDiagnostic,
        &Children,
    )>,
    mut statuses: Query<&mut Text2d, With<TrainStatus>>,
    mut artwork: Query<&mut Sprite, With<TrainArtwork>>,
) {
    for (train, mut transform, mut visibility, mut diagnostic, children) in &mut trains {
        let observation = find_train_snapshot(&snapshot.0, train.train_id)
            .ok_or(PresentationError::MissingTrainSnapshot(train.train_id))
            .and_then(|observed| {
                let position = project_train_position(&observed.state, &layout)
                    .map_err(PresentationError::Projection)?;
                let status = format_train_status(&observed.state, &layout)
                    .map_err(PresentationError::Projection)?;
                Ok((position, status, observed.direction))
            });
        match observation {
            Ok((position, status, direction)) => {
                // The only runtime writer of logical train translation.
                transform.translation = position.extend(4.0);
                for child in children.iter() {
                    if let Ok(mut text) = statuses.get_mut(child) {
                        text.0.clone_from(&status);
                    }
                    if let Ok(mut sprite) = artwork.get_mut(child) {
                        sprite.flip_x = direction == Direction::Backward;
                    }
                }
                *visibility = Visibility::Inherited;
                diagnostic.0 = None;
            }
            Err(error) => {
                if diagnostic.0.as_ref() != Some(&error) {
                    warn!("Train {:?}: {:?}", train.train_id, error);
                    diagnostic.0 = Some(error);
                }
                *visibility = Visibility::Hidden;
            }
        }
    }
}

fn bounce_train(
    time: Res<Time>,
    mut bodies: Query<&mut Transform, (With<TrainBody>, Without<TrainEntity>)>,
) {
    for mut transform in &mut bodies {
        transform.translation.y = (time.elapsed_secs() * 8.0).sin() * 1.5;
    }
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

    fn spawn_test_train(app: &mut App, id: TrainId) -> (Entity, Entity, Entity) {
        let status = app.world_mut().spawn((TrainStatus, Text2d::new(""))).id();
        let body = app
            .world_mut()
            .spawn((
                TrainBody,
                TrainArtwork,
                Sprite::default(),
                Transform::default(),
            ))
            .id();
        let root = app
            .world_mut()
            .spawn((
                TrainEntity { train_id: id },
                PresentationDiagnostic::default(),
                Transform::from_xyz(17.0, 29.0, 0.0),
                Visibility::Hidden,
            ))
            .add_children(&[status, body])
            .id();
        (root, status, body)
    }

    fn observed(id: usize, state: TrainSnapshotState) -> TrainSnapshot {
        TrainSnapshot {
            id: TrainId(id),
            direction: Direction::Backward,
            state,
            velocity: 0,
        }
    }

    #[test]
    fn status_uses_observed_states_and_station_labels() {
        let layout = test_layout();
        for (state, expected) in [
            (
                TrainSnapshotState::Dwelling {
                    station: StationId(42),
                    remaining_seconds: 3,
                },
                "Dwelling at A — 3s",
            ),
            (
                TrainSnapshotState::Ready {
                    station: StationId(7),
                },
                "Ready at B",
            ),
            (moving(42, 7, 1, 3), "Moving A → B — 1/3"),
            (moving(7, 42, 2, 3), "Moving B → A — 2/3"),
        ] {
            assert_eq!(format_train_status(&state, &layout).unwrap(), expected);
        }
        for state in [
            TrainSnapshotState::Dwelling {
                station: StationId(999),
                remaining_seconds: 3,
            },
            TrainSnapshotState::Ready {
                station: StationId(999),
            },
            moving(999, 7, 1, 3),
            moving(42, 999, 1, 3),
        ] {
            assert_eq!(
                format_train_status(&state, &layout),
                Err(ProjectionError::UnknownStation(StationId(999)))
            );
        }
    }

    #[test]
    fn initial_fixture_stays_at_a_across_render_updates() {
        use crate::scenario::{CoreSimulation, initialize_scenario};
        let mut app = App::new();
        initialize_scenario(&mut app);
        let initial = app.world().resource::<LatestSnapshot>().0.clone();
        let layout = StationLayout::from_scenario(app.world().resource::<ScenarioStations>());
        let player = app.world().resource::<PlayerTrain>().0;
        app.insert_resource(layout)
            .add_systems(Update, present_trains);
        let (root, status, _) = spawn_test_train(&mut app, player);
        for _ in 0..20 {
            app.update();
            assert_eq!(
                app.world().get::<Transform>(root).unwrap().translation,
                Vec3::new(-300.0, 0.0, 4.0)
            );
            assert_eq!(
                *app.world().get::<Visibility>(root).unwrap(),
                Visibility::Inherited
            );
            assert_eq!(
                app.world().get::<Text2d>(status).unwrap().0,
                "Dwelling at A — 3s"
            );
            assert_eq!(app.world().resource::<LatestSnapshot>().0, initial);
            assert_eq!(
                app.world().resource::<CoreSimulation>().0.snapshot(),
                initial
            );
        }
    }

    #[test]
    fn driver_publishes_before_presentation_and_root_stays_at_a() {
        use crate::scenario::initialize_scenario;
        use bevy::time::Real;
        use std::time::Duration;

        let mut app = App::new();
        initialize_scenario(&mut app);
        let layout = StationLayout::from_scenario(app.world().resource::<ScenarioStations>());
        let player = app.world().resource::<PlayerTrain>().0;
        app.insert_resource(layout)
            .insert_resource(Time::<Real>::default())
            .add_systems(Update, present_trains.after(crate::timing::drive_core));
        crate::timing::register(&mut app);
        let (root, status, _) = spawn_test_train(&mut app, player);

        for (milliseconds, expected) in [
            (0, "Dwelling at A — 3s"),
            (400, "Dwelling at A — 3s"),
            (600, "Dwelling at A — 2s"),
            (1000, "Dwelling at A — 1s"),
            (1000, "Ready at A"),
            (10_500, "Ready at A"),
        ] {
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(Duration::from_millis(milliseconds));
            app.update();
            assert_eq!(app.world().get::<Text2d>(status).unwrap().0, expected);
            assert_eq!(
                app.world().get::<Transform>(root).unwrap().translation,
                Vec3::new(-300.0, 0.0, 4.0)
            );
            assert_eq!(
                *app.world().get::<Visibility>(root).unwrap(),
                Visibility::Inherited
            );
        }
    }

    #[test]
    fn presentation_needs_only_cached_observations_and_matches_ids() {
        let mut app = App::new();
        app.insert_resource(test_layout())
            .insert_resource(LatestSnapshot(SimulationSnapshot {
                elapsed_seconds: 0,
                trains: vec![
                    observed(
                        9,
                        TrainSnapshotState::Ready {
                            station: StationId(101),
                        },
                    ),
                    observed(
                        42,
                        TrainSnapshotState::Ready {
                            station: StationId(7),
                        },
                    ),
                ],
            }))
            .add_systems(Update, present_trains);
        let (root, status, body) = spawn_test_train(&mut app, TrainId(42));
        for _ in 0..2 {
            app.update();
            assert_eq!(
                app.world().get::<Transform>(root).unwrap().translation,
                Vec3::new(-100.0, 0.0, 4.0)
            );
            assert_eq!(app.world().get::<Text2d>(status).unwrap().0, "Ready at B");
            assert!(app.world().get::<Sprite>(body).unwrap().flip_x);
            app.world_mut()
                .resource_mut::<LatestSnapshot>()
                .0
                .trains
                .reverse();
        }
    }

    #[test]
    fn invalid_observations_hide_without_fabricating_positions_and_recover() {
        let mut app = App::new();
        app.insert_resource(test_layout())
            .insert_resource(LatestSnapshot(SimulationSnapshot {
                elapsed_seconds: 0,
                trains: vec![],
            }))
            .add_systems(Update, present_trains);
        let (root, status, _) = spawn_test_train(&mut app, TrainId(42));
        let invalid = [
            (None, PresentationError::MissingTrainSnapshot(TrainId(42))),
            (
                Some(TrainSnapshotState::Ready {
                    station: StationId(999),
                }),
                PresentationError::Projection(ProjectionError::UnknownStation(StationId(999))),
            ),
            (
                Some(moving(42, 7, 0, 0)),
                PresentationError::Projection(ProjectionError::ZeroTravelDuration),
            ),
            (
                Some(moving(42, 7, 3, 3)),
                PresentationError::Projection(ProjectionError::ElapsedNotBeforeArrival),
            ),
        ];
        for (state, error) in invalid {
            let retained = app.world().get::<Transform>(root).unwrap().translation;
            app.world_mut().resource_mut::<LatestSnapshot>().0.trains =
                state.into_iter().map(|state| observed(42, state)).collect();
            for _ in 0..2 {
                app.update();
                assert_eq!(
                    *app.world().get::<Visibility>(root).unwrap(),
                    Visibility::Hidden
                );
                assert_eq!(
                    app.world().get::<Transform>(root).unwrap().translation,
                    retained
                );
                assert_eq!(
                    app.world()
                        .get::<PresentationDiagnostic>(root)
                        .unwrap()
                        .0
                        .as_ref(),
                    Some(&error)
                );
            }
            app.world_mut().resource_mut::<LatestSnapshot>().0.trains =
                vec![observed(42, moving(7, 42, 1, 3))];
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(root).unwrap(),
                Visibility::Inherited
            );
            assert_position_close(
                app.world()
                    .get::<Transform>(root)
                    .unwrap()
                    .translation
                    .truncate(),
                Vec2::new(-166.666667, 0.0),
            );
            assert_eq!(
                app.world().get::<Text2d>(status).unwrap().0,
                "Moving B → A — 1/3"
            );
            assert!(
                app.world()
                    .get::<PresentationDiagnostic>(root)
                    .unwrap()
                    .0
                    .is_none()
            );
        }
    }

    #[test]
    fn cosmetic_time_moves_only_the_body_child() {
        let mut app = App::new();
        app.insert_resource(test_layout())
            .insert_resource(Time::<()>::default())
            .insert_resource(LatestSnapshot(SimulationSnapshot {
                elapsed_seconds: 0,
                trains: vec![observed(
                    42,
                    TrainSnapshotState::Ready {
                        station: StationId(42),
                    },
                )],
            }))
            .add_systems(Update, (present_trains, bounce_train));
        let (root, _, body) = spawn_test_train(&mut app, TrainId(42));
        app.update();
        let root_position = app.world().get::<Transform>(root).unwrap().translation;
        let body_position = app.world().get::<Transform>(body).unwrap().translation;
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        assert_eq!(
            app.world().get::<Transform>(root).unwrap().translation,
            root_position
        );
        assert_ne!(
            app.world().get::<Transform>(body).unwrap().translation.y,
            body_position.y
        );
    }

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
