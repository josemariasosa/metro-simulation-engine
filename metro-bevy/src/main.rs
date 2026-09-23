use bevy::prelude::*;

#[derive(Component)]
struct Train {
    direction: f32,
    dwell_remaining: f32,
}

#[derive(Component)]
struct TrainBody;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, (move_train, bounce_train))
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2d);

    commands
        .spawn((
            Train {
                direction: 1.0,
                dwell_remaining: 0.0,
            },
            Transform::from_xyz(-300.0, 0.0, 0.0),
        ))
        .with_children(|parent| {
            // Base + wheels
            parent.spawn((
                Sprite::from_image(asset_server.load("trains/train_base.png")),
                Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(0.25)),
            ));

            // Body
            parent.spawn((
                TrainBody,
                Sprite::from_image(asset_server.load("trains/train_body.png")),
                Transform::from_xyz(0.0, 0.0, 1.0).with_scale(Vec3::splat(0.25)),
            ));
        });
}

fn bounce_train(time: Res<Time>, mut query: Query<&mut Transform, With<TrainBody>>) {
    for mut transform in &mut query {
        let bounce = (time.elapsed_secs() * 8.0).sin() * 1.5;
        transform.translation.y = bounce;
    }
}

fn move_train(time: Res<Time>, mut query: Query<(&mut Transform, &mut Train)>) {
    const LEFT: f32 = -300.0;
    const RIGHT: f32 = 300.0;
    const SPEED: f32 = 100.0;
    const DWELL_TIME: f32 = 1.0;

    for (mut transform, mut train) in &mut query {
        // If we're dwelling, stay put.
        if train.dwell_remaining > 0.0 {
            train.dwell_remaining -= time.delta_secs();
            continue;
        }

        // Otherwise, move.
        transform.translation.x += SPEED * train.direction * time.delta_secs();

        // Arrived at right terminal.
        if transform.translation.x >= RIGHT {
            transform.translation.x = RIGHT;
            train.direction = -1.0;
            train.dwell_remaining = DWELL_TIME;

            transform.scale.x = -1.0;
        }

        // Arrived at left terminal.
        if transform.translation.x <= LEFT {
            transform.translation.x = LEFT;
            train.direction = 1.0;
            train.dwell_remaining = DWELL_TIME;

            transform.scale.x = 1.0;
        }
    }
}
