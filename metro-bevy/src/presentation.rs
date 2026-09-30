use bevy::prelude::*;
use metro_core::station::StationId;

use crate::scenario::ScenarioStations;

struct StationPresentation {
    // Retained for later ID-based snapshot projection.
    id: StationId,
    label: &'static str,
    position: Vec2,
}

/// Drawing metadata only: these positions do not define railway topology or timing.
#[derive(Resource)]
struct StationLayout([StationPresentation; 4]);

impl StationLayout {
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
