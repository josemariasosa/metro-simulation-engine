use bevy::prelude::*;

mod presentation;
mod scenario;

fn main() {
    let mut app = App::new();
    scenario::initialize_scenario(&mut app);
    presentation::register(&mut app);

    app.add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
