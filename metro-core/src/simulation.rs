use std::collections::HashSet;

use crate::command::{CommandError, TrainCommand};
use crate::domain::departure::{AutomaticDepartureDecision, AutomaticDepartureProposal};
use crate::domain::departure::{can_admit_departure, select_departure_candidate};
use crate::domain::resource::ResourceView;
use crate::dwell::DwellPolicy;
use crate::network::Network;
use crate::snapshot::{SimulationSnapshot, TrainSnapshot, TrainSnapshotState};
use crate::station::StationId;
use crate::train::{AtStationState, Train, TrainControl, TrainState};

#[derive(Debug)]
pub struct Simulation {
    pub elapsed_seconds: u64,
    pub network: Network,
    trains: Vec<Train>,
    dwell_policy: DwellPolicy,
}

fn dwelling_snapshot_state(
    station: StationId,
    elapsed_seconds: u64,
    dwell_seconds: u64,
) -> TrainSnapshotState {
    assert!(
        elapsed_seconds < dwell_seconds,
        "dwelling elapsed time must be less than dwell duration"
    );

    TrainSnapshotState::Dwelling {
        station,
        remaining_seconds: dwell_seconds - elapsed_seconds,
    }
}

impl Simulation {
    pub fn new(network: Network, trains: Vec<Train>, dwell_policy: DwellPolicy) -> Self {
        let mut ids = HashSet::new();
        for train in &trains {
            assert!(ids.insert(train.id), "duplicate TrainId");
        }
        let _ = ResourceView::derive(&trains);

        Self {
            elapsed_seconds: 0,
            network,
            trains,
            dwell_policy,
        }
    }

    /// Applies a command without advancing time. Rejections leave state unchanged.
    pub fn apply_command(&mut self, command: TrainCommand) -> Result<(), CommandError> {
        match command {
            TrainCommand::Accelerate { train_id } => {
                let index = self
                    .trains
                    .iter()
                    .position(|train| train.id == train_id)
                    .ok_or(CommandError::UnknownTrain)?;
                let train = &self.trains[index];

                if train.control != TrainControl::Manual {
                    return Err(CommandError::NotManual);
                }

                let station = match train.state {
                    TrainState::Moving { .. } => return Ok(()),
                    TrainState::AtStation { station, .. } => station,
                };
                let candidate = select_departure_candidate(&self.network, station, train.direction)
                    .ok_or(CommandError::NoOutgoingTrack)?;
                let resources = ResourceView::derive(&self.trains);
                if !can_admit_departure(&candidate, &resources) {
                    return Err(CommandError::Blocked);
                }
                self.trains[index].apply_departure(candidate);
                Ok(())
            }
        }
    }

    /// Returns an owned observation of the current state in train vector order.
    pub fn snapshot(&self) -> SimulationSnapshot {
        SimulationSnapshot {
            elapsed_seconds: self.elapsed_seconds,
            trains: self
                .trains
                .iter()
                .map(|train| self.snapshot_train(train))
                .collect(),
        }
    }

    fn snapshot_train(&self, train: &Train) -> TrainSnapshot {
        TrainSnapshot {
            id: train.id,
            direction: train.direction,
            velocity: train.velocity,
            state: self.snapshot_train_state(&train.state),
        }
    }

    fn snapshot_train_state(&self, state: &TrainState) -> TrainSnapshotState {
        match state {
            TrainState::AtStation {
                station,
                state:
                    AtStationState::Dwelling {
                        elapsed_seconds,
                        dwell_seconds,
                    },
            } => dwelling_snapshot_state(*station, *elapsed_seconds, *dwell_seconds),
            TrainState::AtStation {
                station,
                state: AtStationState::Ready,
            } => TrainSnapshotState::Ready { station: *station },
            TrainState::Moving {
                from,
                to,
                elapsed_seconds,
            } => {
                let track = self
                    .network
                    .track(*from, *to)
                    .expect("moving train must reference an existing track");

                TrainSnapshotState::Moving {
                    from: *from,
                    to: *to,
                    elapsed_seconds: *elapsed_seconds,
                    travel_seconds: track.travel_seconds,
                }
            }
        }
    }

    fn step_at_station(train: &mut Train, station: StationId, state: AtStationState) {
        match state {
            AtStationState::Dwelling {
                elapsed_seconds,
                dwell_seconds,
            } => {
                if elapsed_seconds + 1 < dwell_seconds {
                    train.state = TrainState::AtStation {
                        station,
                        state: AtStationState::Dwelling {
                            elapsed_seconds: elapsed_seconds + 1,
                            dwell_seconds,
                        },
                    };
                } else {
                    train.state = TrainState::AtStation {
                        station,
                        state: AtStationState::Ready,
                    };
                    train.velocity = 0;
                }
            }
            AtStationState::Ready => {
                // Intentionally no train-state change while waiting for input.
            }
        }
    }

    fn step_moving(
        network: &Network,
        dwell_policy: &DwellPolicy,
        train: &mut Train,
        from: StationId,
        to: StationId,
        elapsed_seconds: u64,
    ) {
        let track = network.track(from, to).expect("TRACK_NOT_FOUND");
        if elapsed_seconds + 1 >= track.travel_seconds {
            let dwell_seconds = dwell_policy.dwell_seconds(to, train);
            train.state = TrainState::AtStation {
                station: to,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds,
                },
            };
            train.velocity = 0;
        } else {
            train.state = TrainState::Moving {
                from,
                to,
                elapsed_seconds: elapsed_seconds + 1,
            };
        }
    }

    pub fn step(&mut self) {
        // World N stays immutable until every automatic departure is resolved.
        let starting_resources = ResourceView::derive(&self.trains);
        let mut proposals = Vec::new();
        for (train_index, train) in self.trains.iter().enumerate() {
            if train.control != TrainControl::Automatic {
                continue;
            }
            let TrainState::AtStation { station, state } = train.state else {
                continue;
            };
            let eligible = match state {
                AtStationState::Ready => true,
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                } => elapsed_seconds + 1 >= dwell_seconds,
            };
            if eligible {
                proposals.push(AutomaticDepartureProposal {
                    train_index,
                    train_id: train.id,
                    candidate: select_departure_candidate(&self.network, station, train.direction)
                        .expect("NO_NEXT_TRACK_AFTER_REVERSING_DIRECTION"),
                });
            }
        }
        proposals.sort_unstable_by_key(|proposal| proposal.train_id.0);

        let mut accepted_tracks = HashSet::new();
        let mut accepted_destination_slots = HashSet::new();
        let mut decisions: Vec<_> = self
            .trains
            .iter()
            .map(|_| AutomaticDepartureDecision::NotEligible)
            .collect();
        for proposal in proposals {
            let candidate = proposal.candidate;
            let track = (candidate.from, candidate.to);
            let slot = (candidate.to, candidate.direction);
            decisions[proposal.train_index] =
                if can_admit_departure(&candidate, &starting_resources)
                    && !accepted_tracks.contains(&track)
                    && !accepted_destination_slots.contains(&slot)
                {
                    accepted_tracks.insert(track);
                    accepted_destination_slots.insert(slot);
                    AutomaticDepartureDecision::Accepted(candidate)
                } else {
                    AutomaticDepartureDecision::Rejected {
                        station: candidate.from,
                    }
                };
        }

        // Decisions are fixed: no train can use a resource released in this pass.
        for (train, decision) in self.trains.iter_mut().zip(decisions) {
            match decision {
                AutomaticDepartureDecision::Accepted(candidate) => {
                    train.apply_departure(candidate);
                    continue;
                }
                AutomaticDepartureDecision::Rejected { station } => {
                    train.state = TrainState::AtStation {
                        station,
                        state: AtStationState::Ready,
                    };
                    train.velocity = 0;
                    continue;
                }
                AutomaticDepartureDecision::NotEligible => {}
            }
            match train.state {
                TrainState::AtStation { station, state } => {
                    Self::step_at_station(train, station, state);
                }

                TrainState::Moving {
                    from,
                    to,
                    elapsed_seconds,
                } => {
                    Self::step_moving(
                        &self.network,
                        &self.dwell_policy,
                        train,
                        from,
                        to,
                        elapsed_seconds,
                    );
                }
            }
        }
        self.elapsed_seconds += 1;
    }

    pub fn trains(&self) -> &[Train] {
        &self.trains
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::utils::moving_train;
    use crate::train::{Direction, Train, TrainId, TrainState};

    use super::*;

    fn assert_blocked_unchanged(simulation: &mut Simulation, train_id: TrainId) {
        let before = simulation.snapshot();
        let trains = simulation.trains.clone();
        let resources = ResourceView::derive(&simulation.trains);
        assert_eq!(
            simulation.apply_command(TrainCommand::Accelerate { train_id }),
            Err(CommandError::Blocked)
        );
        assert_eq!(simulation.snapshot(), before);
        assert_eq!(simulation.trains.len(), trains.len());
        for (actual, expected) in simulation.trains.iter().zip(&trains) {
            assert_eq!(actual.id, expected.id);
            assert_eq!(actual.capacity, expected.capacity);
            assert_eq!(actual.control, expected.control);
            assert_eq!(actual.state, expected.state);
            assert_eq!(actual.direction, expected.direction);
            assert_eq!(actual.velocity, expected.velocity);
        }
        assert_eq!(ResourceView::derive(&simulation.trains), resources);
    }

    #[test]
    fn occupied_track_and_reserved_destination_block_manual_departure_exactly() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);
        let train = Train::new_manual(
            TrainId(1),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let mut blocker = moving_train(2, a, b, Direction::Forward);
        blocker.state = TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 4,
        };
        let mut simulation = Simulation::new(network, vec![train, blocker], dwell_policy);
        let resources = ResourceView::derive(simulation.trains());
        assert!(!resources.track_available(a, b));
        assert!(!resources.station_slot_available(b, Direction::Forward));
        assert_blocked_unchanged(&mut simulation, TrainId(1));
    }

    #[test]
    fn occupied_destination_blocks_during_dwell_and_after_ready_exactly() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 2);
        let trains = vec![
            Train::new_manual(
                TrainId(1),
                100,
                a,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
            Train::new_manual(
                TrainId(2),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        ];
        let mut simulation = Simulation::new(network, trains, dwell_policy);
        for elapsed in 0..=3 {
            assert_eq!(simulation.elapsed_seconds, elapsed);
            assert_blocked_unchanged(&mut simulation, TrainId(1));
            if elapsed < 3 {
                simulation.step();
            }
        }
        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: a,
                state: AtStationState::Ready,
            }
        );
    }

    #[test]
    fn blocked_terminal_reversal_preserves_direction_until_fresh_command() {
        let SimulationFixture {
            network,
            stations: [a, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            Train::new_manual(
                TrainId(1),
                100,
                c,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
            Train::new_manual(
                TrainId(2),
                100,
                b,
                Direction::Backward,
                DwellPolicy::default_dwell_seconds(),
            ),
        ];
        let mut simulation = Simulation::new(network, trains, dwell_policy);
        simulation.step();
        assert_blocked_unchanged(&mut simulation, TrainId(1));
        assert_eq!(simulation.trains()[0].direction, Direction::Forward);
        simulation
            .apply_command(TrainCommand::Accelerate {
                train_id: TrainId(2),
            })
            .unwrap();
        assert_eq!(
            simulation.trains()[1].state,
            TrainState::Moving {
                from: b,
                to: a,
                elapsed_seconds: 0,
            }
        );
        simulation.step();
        assert_eq!(simulation.trains()[0].direction, Direction::Forward);
        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: c,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 2,
                    dwell_seconds: 3
                },
            }
        );
        simulation
            .apply_command(TrainCommand::Accelerate {
                train_id: TrainId(1),
            })
            .unwrap();
        assert_eq!(simulation.elapsed_seconds, 2);
        assert_eq!(simulation.trains()[0].direction, Direction::Backward);
        assert_eq!(simulation.trains()[0].velocity, 1);
        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: c,
                to: b,
                elapsed_seconds: 0,
            }
        );
        ResourceView::derive(simulation.trains());
    }

    #[test]
    fn physical_blocking_of_existing_track_never_selects_reverse_fallback() {
        let SimulationFixture {
            network,
            stations: [a, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            Train::new_manual(
                TrainId(1),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
            moving_train(2, b, c, Direction::Forward),
        ];
        let mut simulation = Simulation::new(network, trains, dwell_policy);
        let resources = ResourceView::derive(simulation.trains());
        let reverse =
            select_departure_candidate(&simulation.network, b, Direction::Backward).unwrap();
        assert_eq!(reverse.to, a);
        assert!(can_admit_departure(&reverse, &resources));
        assert_blocked_unchanged(&mut simulation, TrainId(1));
    }

    #[test]
    fn opposite_destination_slots_and_reverse_tracks_do_not_block_manual_departure() {
        for blocker_kind in 0..3 {
            let SimulationFixture {
                network,
                stations: [a, b, c],
                dwell_policy,
            } = SimulationFixture::new(["A", "B", "C"], 2);
            let train = Train::new_manual(
                TrainId(1),
                100,
                a,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            );
            let blocker = match blocker_kind {
                0 => Train::new_manual(
                    TrainId(2),
                    100,
                    b,
                    Direction::Backward,
                    DwellPolicy::default_dwell_seconds(),
                ),
                1 => moving_train(2, c, b, Direction::Backward),
                _ => moving_train(2, b, a, Direction::Backward),
            };
            let mut simulation = Simulation::new(network, vec![train, blocker], dwell_policy);
            let before = simulation.snapshot();
            simulation
                .apply_command(TrainCommand::Accelerate {
                    train_id: TrainId(1),
                })
                .unwrap();
            assert_eq!(simulation.elapsed_seconds, before.elapsed_seconds);
            assert_eq!(simulation.snapshot().trains[1], before.trains[1]);
            assert_eq!(
                simulation.trains()[0].state,
                TrainState::Moving {
                    from: a,
                    to: b,
                    elapsed_seconds: 0,
                }
            );
            assert_eq!(simulation.trains()[0].direction, Direction::Forward);
            assert_eq!(simulation.trains()[0].velocity, 1);
            ResourceView::derive(simulation.trains());
        }
    }

    /// A fresh bidirectional line for each test. Station IDs follow the supplied order.
    struct SimulationFixture<const N: usize> {
        network: Network,
        stations: [StationId; N],
        dwell_policy: DwellPolicy,
    }

    impl<const N: usize> SimulationFixture<N> {
        fn new(station_names: [&str; N], travel_seconds: u64) -> Self {
            let mut network = Network::new();
            let stations = station_names.map(|name| network.add_station(name));
            for pair in stations.windows(2) {
                network.connect_bidirectional(pair[0], pair[1], travel_seconds);
            }

            Self {
                network,
                stations,
                dwell_policy: DwellPolicy::new(),
            }
        }
    }

    #[test]
    #[should_panic(expected = "duplicate TrainId")]
    fn simulation_rejects_duplicate_train_ids() {
        let SimulationFixture {
            network,
            stations: [a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            Train::new(
                TrainId(42),
                100,
                a,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
            Train::new(
                TrainId(42),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    #[should_panic(expected = "station slot already occupied")]
    fn simulation_rejects_duplicate_station_slot_occupancy() {
        let SimulationFixture {
            network,
            stations: [_a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            Train::new(
                TrainId(1),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
            Train::new(
                TrainId(2),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    #[should_panic(expected = "directed track already occupied")]
    fn simulation_rejects_duplicate_directed_track_occupancy() {
        let SimulationFixture {
            network,
            stations: [a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            moving_train(1, a, b, Direction::Forward),
            moving_train(2, a, b, Direction::Forward),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    #[should_panic(expected = "directed track already occupied")]
    fn simulation_rejects_duplicate_destination_reservations() {
        let SimulationFixture {
            network,
            stations: [a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        // In this linear topology, competing valid reservations also share a track.
        // The private reservation test above isolates the reservation conflict.
        let trains = vec![
            moving_train(1, a, b, Direction::Forward),
            moving_train(2, a, b, Direction::Forward),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    #[should_panic(expected = "station slot occupied and reserved")]
    fn simulation_rejects_occupied_and_reserved_slot() {
        let SimulationFixture {
            network,
            stations: [a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            Train::new(
                TrainId(1),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
            moving_train(2, a, b, Direction::Forward),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    #[should_panic(expected = "station slot occupied and reserved")]
    fn simulation_rejects_reserved_and_occupied_slot() {
        let SimulationFixture {
            network,
            stations: [a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            moving_train(2, a, b, Direction::Forward),
            Train::new(
                TrainId(1),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    fn simulation_accepts_opposite_station_slots() {
        let SimulationFixture {
            network,
            stations: [_a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            Train::new(
                TrainId(1),
                100,
                b,
                Direction::Forward,
                DwellPolicy::default_dwell_seconds(),
            ),
            Train::new(
                TrainId(2),
                100,
                b,
                Direction::Backward,
                DwellPolicy::default_dwell_seconds(),
            ),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    fn simulation_accepts_opposite_directed_tracks() {
        let SimulationFixture {
            network,
            stations: [a, b, _c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            moving_train(1, a, b, Direction::Forward),
            moving_train(2, b, a, Direction::Backward),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    #[test]
    fn simulation_accepts_opposite_destination_reservations() {
        let SimulationFixture {
            network,
            stations: [a, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);
        let trains = vec![
            moving_train(1, a, b, Direction::Forward),
            moving_train(2, c, b, Direction::Backward),
        ];
        Simulation::new(network, trains, dwell_policy);
    }

    /// Default scenario: one train dwelling at A, with a ten-second track to B.
    fn test_simulation() -> Simulation {
        let SimulationFixture {
            network,
            stations: [a, _],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);
        let train = Train::new(
            TrainId(0),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        Simulation::new(network, vec![train], dwell_policy)
    }

    #[test]
    fn simulation_advances_time() {
        let network = Network::new();
        let dwell_policy = DwellPolicy::new();
        let trains = Vec::new();

        let mut simulation = Simulation::new(network, trains, dwell_policy);

        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.elapsed_seconds, 3);
    }

    #[test]
    fn new_train_starts_dwelling_and_advances_dwell_time() {
        let SimulationFixture {
            network,
            stations: [station_a, _],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 60);

        let train = Train::new(
            TrainId(0),
            100,
            station_a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );

        let mut simulation = Simulation::new(network, vec![train.clone()], dwell_policy.clone());

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: station_a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
                }
            }
        );

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: station_a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 1,
                    dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
                }
            }
        );
    }

    #[test]
    fn train_remains_dwelling_until_dwell_time_is_reached() {
        let SimulationFixture {
            network,
            stations: [station_a, station_b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);

        let train = Train::new(
            TrainId(0),
            100,
            station_a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let mut simulation = Simulation::new(network, vec![train.clone()], dwell_policy.clone());

        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: station_a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 2,
                    dwell_seconds: dwell_policy.dwell_seconds(station_a, &train),
                },
            }
        );

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: station_a,
                to: station_b,
                elapsed_seconds: 0,
            }
        );
    }

    #[test]
    fn reverse_candidate_changes_train_only_when_committed() {
        let SimulationFixture {
            network,
            stations: [_, b, c],
            ..
        } = SimulationFixture::new(["A", "B", "C"], 10);
        let mut train = Train::new_manual(
            TrainId(42),
            100,
            c,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let before = train.clone();
        let TrainState::AtStation { station, .. } = train.state else {
            panic!("expected station state");
        };
        let candidate = select_departure_candidate(&network, station, train.direction).unwrap();

        assert_eq!(candidate.from, c);
        assert_eq!(candidate.to, b);
        assert_eq!(candidate.direction, Direction::Backward);
        assert_eq!(train.id, before.id);
        assert_eq!(train.capacity, before.capacity);
        assert_eq!(train.control, before.control);
        assert_eq!(train.state, before.state);
        assert_eq!(train.direction, before.direction);
        assert_eq!(train.velocity, before.velocity);

        train.apply_departure(candidate);

        assert_eq!(train.id, before.id);
        assert_eq!(train.capacity, before.capacity);
        assert_eq!(train.control, before.control);
        assert_eq!(train.direction, Direction::Backward);
        assert_eq!(train.velocity, 1);
        assert_eq!(
            train.state,
            TrainState::Moving {
                from: c,
                to: b,
                elapsed_seconds: 0
            }
        );
    }

    #[test]
    fn automatic_departure_selects_current_direction_or_reverse_fallback() {
        for (station, direction, gap, to, selected_direction) in [
            (1, Direction::Forward, false, 2, Direction::Forward),
            (1, Direction::Backward, false, 0, Direction::Backward),
            (2, Direction::Forward, false, 1, Direction::Backward),
            (0, Direction::Backward, false, 1, Direction::Forward),
            (1, Direction::Forward, true, 0, Direction::Backward),
        ] {
            let mut network = Network::new();
            let a = network.add_station("A");
            let b = network.add_station("B");
            let c = network.add_station("C");
            network.connect_bidirectional(a, b, 10);
            if !gap {
                network.connect_bidirectional(b, c, 10);
            }
            let train = Train::new(
                TrainId(42),
                100,
                StationId(station),
                direction,
                DwellPolicy::default_dwell_seconds(),
            );
            let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());

            for elapsed_seconds in 0..3 {
                assert_eq!(simulation.elapsed_seconds, elapsed_seconds);
                let train = &simulation.trains()[0];
                assert_eq!(train.direction, direction);
                assert_eq!(train.velocity, 0);
                assert_eq!(
                    train.state,
                    TrainState::AtStation {
                        station: StationId(station),
                        state: AtStationState::Dwelling {
                            elapsed_seconds,
                            dwell_seconds: 3
                        },
                    }
                );
                simulation.step();
            }

            let train = &simulation.trains()[0];
            assert_eq!(simulation.elapsed_seconds, 3);
            assert_eq!(train.direction, selected_direction);
            assert_eq!(train.velocity, 1);
            assert_eq!(
                train.state,
                TrainState::Moving {
                    from: StationId(station),
                    to: StationId(to),
                    elapsed_seconds: 0,
                }
            );
        }
    }

    #[test]
    #[should_panic(expected = "NO_NEXT_TRACK_AFTER_REVERSING_DIRECTION")]
    fn automatic_departure_without_outgoing_track_preserves_panic() {
        let mut network = Network::new();
        let station = network.add_station("Isolated");
        let train = Train::new(
            TrainId(0),
            100,
            station,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let mut simulation = Simulation::new(network, vec![train], DwellPolicy::new());
        for _ in 0..3 {
            simulation.step();
        }
    }

    #[test]
    fn automatic_ready_train_retries_departure_on_next_step() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);
        let mut train = Train::new(
            TrainId(0),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        train.state = TrainState::AtStation {
            station: a,
            state: AtStationState::Ready,
        };
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);
        simulation.step();
        assert_eq!(simulation.elapsed_seconds, 1);
        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );
        assert_eq!(simulation.snapshot().trains[0].velocity, 1);
    }

    #[test]
    fn automatic_train_starts_moving_with_velocity_one() {
        let SimulationFixture {
            network,
            stations: [station_a, station_b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 60);

        let train = Train::new(
            TrainId(0),
            100,
            station_a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        assert_eq!(train.control, crate::train::TrainControl::Automatic);

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // The train stays stopped through the first two seconds of dwell.
        for elapsed_seconds in 0..3 {
            let train = &simulation.trains()[0];
            assert_eq!(
                (train.state, train.velocity),
                (
                    TrainState::AtStation {
                        station: station_a,
                        state: AtStationState::Dwelling {
                            elapsed_seconds,
                            dwell_seconds: 3,
                        },
                    },
                    0,
                )
            );
            simulation.step();
        }

        // Departure sets velocity before any track traversal time is consumed.
        let train = &simulation.trains()[0];
        assert_eq!(
            (train.state, train.velocity),
            (
                TrainState::Moving {
                    from: station_a,
                    to: station_b,
                    elapsed_seconds: 0,
                },
                1,
            )
        );
    }

    #[test]
    fn step_at_station_preserves_configured_dwell_duration() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 10);

        let mut train = Train::new(
            TrainId(0),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        train.state = TrainState::AtStation {
            station: a,
            state: AtStationState::Dwelling {
                elapsed_seconds: 0,
                dwell_seconds: 5,
            },
        };

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        simulation.step();
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: a,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 4,
                    dwell_seconds: 5,
                },
            }
        );

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );
    }

    #[test]
    fn moving_train_keeps_velocity_one_during_traversal() {
        let SimulationFixture {
            network,
            stations: [station_a, station_b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 60);

        let mut train = Train::new(
            TrainId(0),
            100,
            station_a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        train.state = TrainState::Moving {
            from: station_a,
            to: station_b,
            elapsed_seconds: 1,
        };
        train.velocity = 1;

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        simulation.step();

        assert_eq!(simulation.trains()[0].velocity, 1);
        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: station_a,
                to: station_b,
                elapsed_seconds: 2,
            }
        );
    }

    #[test]
    fn moving_train_advances_from_zero_elapsed_seconds() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 3);

        let mut train = Train::new(
            TrainId(0),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        train.state = TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0,
        };
        train.velocity = 1;

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 1,
            }
        );
    }

    #[test]
    fn arriving_train_stops_and_starts_fresh_dwell() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 1);

        let mut train = Train::new(
            TrainId(0),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        train.state = TrainState::Moving {
            from: a,
            to: b,
            elapsed_seconds: 0,
        };
        train.velocity = 1;

        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        simulation.step();

        let train = &simulation.trains()[0];
        assert_eq!(train.velocity, 0);
        assert_eq!(
            train.state,
            TrainState::AtStation {
                station: b,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: 3,
                },
            }
        );
    }

    #[test]
    fn train_remains_moving_until_track_travel_time_is_reached() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 3);

        let train = Train::new(
            TrainId(0),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at A for 3 seconds.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );

        // Travel for 2 of the required 3 seconds.
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 2,
            }
        );
    }

    #[test]
    fn train_arrives_when_track_travel_time_is_reached() {
        let SimulationFixture {
            network,
            stations: [a, b],
            dwell_policy,
        } = SimulationFixture::new(["A", "B"], 3);

        let train = Train::new(
            TrainId(0),
            100,
            a,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let expected_dwell_seconds = dwell_policy.dwell_seconds(b, &train);
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at A for 3 seconds.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: a,
                to: b,
                elapsed_seconds: 0,
            }
        );

        // Travel from A to B for 3 seconds.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: b,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: expected_dwell_seconds,
                },
            }
        );
    }

    #[test]
    fn train_reverses_direction_at_end_of_line() {
        let SimulationFixture {
            network,
            stations: [_, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 10);

        let train = Train::new(
            TrainId(0),
            100,
            c,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at C for 3 seconds.
        assert_eq!(simulation.trains()[0].velocity, 0);
        simulation.step();
        simulation.step();
        assert_eq!(simulation.trains()[0].velocity, 0);
        simulation.step();

        // No track exists forward from C, so the train reverses
        // and starts moving toward B.
        assert_eq!(simulation.trains()[0].direction, Direction::Backward);
        assert_eq!(simulation.trains()[0].velocity, 1);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: c,
                to: b,
                elapsed_seconds: 0,
            }
        );
    }

    #[test]
    fn train_continues_in_reversed_direction_after_reaching_endpoint() {
        let SimulationFixture {
            network,
            stations: [a, b, c],
            dwell_policy,
        } = SimulationFixture::new(["A", "B", "C"], 2);

        let train = Train::new(
            TrainId(0),
            100,
            c,
            Direction::Forward,
            DwellPolicy::default_dwell_seconds(),
        );
        let expected_dwell_seconds = dwell_policy.dwell_seconds(b, &train);
        let mut simulation = Simulation::new(network, vec![train], dwell_policy);

        // Dwell at C, then reverse and depart toward B.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.trains()[0].direction, Direction::Backward);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: c,
                to: b,
                elapsed_seconds: 0,
            }
        );

        // Travel C -> B.
        simulation.step();
        simulation.step();

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::AtStation {
                station: b,
                state: AtStationState::Dwelling {
                    elapsed_seconds: 0,
                    dwell_seconds: expected_dwell_seconds,
                },
            }
        );

        // Dwell at B, then continue toward A.
        simulation.step();
        simulation.step();
        simulation.step();

        assert_eq!(simulation.trains()[0].direction, Direction::Backward);

        assert_eq!(
            simulation.trains()[0].state,
            TrainState::Moving {
                from: b,
                to: a,
                elapsed_seconds: 0,
            }
        );
    }

    #[test]
    fn initial_dwelling_state_maps_to_snapshot_state() {
        let simulation = test_simulation();
        let train = &simulation.trains()[0];

        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to start dwelling");
        };

        let snapshot_state = dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds);

        assert_eq!(
            snapshot_state,
            TrainSnapshotState::Dwelling {
                station: StationId(0),
                remaining_seconds: 3,
            }
        );
    }

    #[test]
    fn dwelling_snapshot_countdown_matches_future_steps_until_departure() {
        let mut simulation = test_simulation();

        let train = &simulation.trains()[0];

        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to start dwelling");
        };

        assert_eq!(
            dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds,),
            TrainSnapshotState::Dwelling {
                station,
                remaining_seconds: 3,
            }
        );

        simulation.step();

        let train = &simulation.trains()[0];
        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to still be dwelling");
        };

        assert_eq!(
            dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds,),
            TrainSnapshotState::Dwelling {
                station,
                remaining_seconds: 2,
            }
        );

        simulation.step();

        let train = &simulation.trains()[0];
        let TrainState::AtStation {
            station,
            state:
                AtStationState::Dwelling {
                    elapsed_seconds,
                    dwell_seconds,
                },
        } = train.state
        else {
            panic!("expected train to still be dwelling");
        };

        assert_eq!(
            dwelling_snapshot_state(station, elapsed_seconds, dwell_seconds,),
            TrainSnapshotState::Dwelling {
                station,
                remaining_seconds: 1,
            }
        );

        simulation.step();

        assert!(matches!(
            simulation.trains()[0].state,
            TrainState::Moving { .. }
        ));
    }

    #[test]
    fn dwelling_snapshot_uses_stored_dwell_duration() {
        let snapshot = dwelling_snapshot_state(StationId(0), 2, 5);

        assert_eq!(
            snapshot,
            TrainSnapshotState::Dwelling {
                station: StationId(0),
                remaining_seconds: 3,
            }
        );
    }
}
