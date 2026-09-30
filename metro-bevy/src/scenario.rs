use bevy::prelude::{App, Resource};
use metro_core::dwell::DwellPolicy;
use metro_core::network::Network;
use metro_core::simulation::Simulation;
use metro_core::snapshot::SimulationSnapshot;
use metro_core::station::StationId;
use metro_core::train::{Direction, Train, TrainId};

#[derive(Resource)]
pub(super) struct CoreSimulation(pub(super) Simulation);

#[derive(Resource)]
pub(super) struct LatestSnapshot(pub(super) SimulationSnapshot);

#[derive(Resource)]
pub(super) struct PlayerTrain(pub(super) TrainId);

pub(super) struct ScenarioStation {
    pub(super) id: StationId,
    pub(super) label: &'static str,
}

#[derive(Resource)]
pub(super) struct ScenarioStations(pub(super) [ScenarioStation; 4]);

/// Own the dormant core scenario and capture its initial presentation snapshot.
pub(super) fn initialize_scenario(app: &mut App) {
    let mut network = Network::new();
    let stations = ["A", "B", "C", "D"].map(|label| ScenarioStation {
        id: network.add_station(label),
        label,
    });
    let [a, b, c, d] = stations.each_ref().map(|station| station.id);

    network.connect_bidirectional(a, b, 3);
    network.connect_bidirectional(b, c, 3);
    network.connect_bidirectional(c, d, 3);

    let player_id = TrainId(0);
    let train = Train::new_manual(player_id, 100, a, Direction::Forward);
    let simulation = Simulation::new(network, vec![train], DwellPolicy::new());
    let snapshot = simulation.snapshot();

    app.insert_resource(CoreSimulation(simulation))
        .insert_resource(LatestSnapshot(snapshot))
        .insert_resource(PlayerTrain(player_id))
        .insert_resource(ScenarioStations(stations));
}

#[cfg(test)]
mod tests {
    use super::*;
    use metro_core::snapshot::TrainSnapshotState;

    #[test]
    fn initial_resources_capture_manual_scenario() {
        let mut app = App::new();
        initialize_scenario(&mut app);

        // Inspect immediately: no app update or simulation step is needed.
        let world = app.world();
        let simulation = &world.resource::<CoreSimulation>().0;
        let snapshot = &world.resource::<LatestSnapshot>().0;
        let player_id = world.resource::<PlayerTrain>().0;
        let stations = &world.resource::<ScenarioStations>().0;

        assert_eq!(
            stations
                .each_ref()
                .map(|station| (station.id, station.label)),
            [
                (StationId(0), "A"),
                (StationId(1), "B"),
                (StationId(2), "C"),
                (StationId(3), "D"),
            ]
        );
        assert_eq!(player_id, TrainId(0));
        assert_eq!(snapshot.trains.len(), 1);
        let train = snapshot
            .trains
            .iter()
            .find(|train| train.id == player_id)
            .expect("configured player must be present in the initial snapshot");
        assert_eq!(
            train.state,
            TrainSnapshotState::Dwelling {
                station: stations[0].id,
                remaining_seconds: 3,
            }
        );
        assert_eq!(train.direction, Direction::Forward);
        assert_eq!(train.velocity, 0);
        assert_eq!(simulation.elapsed_seconds, 0);
        assert_eq!(snapshot.elapsed_seconds, 0);
        assert_eq!(*snapshot, simulation.snapshot());

        assert_eq!(simulation.network.station_count(), 4);
        for pair in stations.windows(2) {
            let (from, to) = (pair[0].id, pair[1].id);
            for (from, to) in [(from, to), (to, from)] {
                assert_eq!(
                    simulation
                        .network
                        .track(from, to)
                        .expect("adjacent stations must be connected in both directions")
                        .travel_seconds,
                    3
                );
            }
        }
        let core_train = simulation
            .trains()
            .iter()
            .find(|train| train.id == player_id)
            .expect("configured player must be owned by the simulation");
        assert_eq!(core_train.capacity, 100);
    }
}
