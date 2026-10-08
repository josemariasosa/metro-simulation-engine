use crate::command::{CommandError, TrainCommand};
use crate::domain::constraint::{
    ConstraintError, ConstraintId, ConstraintOrigin, ConstraintRecord, ConstraintView,
    OperationalConstraint,
};
use crate::domain::departure::{
    AutomaticDepartureDecision, AutomaticDepartureProposal, can_admit_departure,
    select_departure_candidate,
};
use crate::domain::dwell::DwellPolicy;
use crate::domain::network::Network;
use crate::domain::resource::ResourceView;
use crate::domain::station::StationId;
use crate::domain::train::{AtStationState, Train, TrainId, TrainState};
use crate::snapshot::{ConstraintSnapshot, SimulationSnapshot, TrainSnapshot, TrainSnapshotState};
use std::collections::HashSet;

pub(crate) use train_entity::TrainEntity;

mod train_entity;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddTrainError {
    ResourceConflict,
    IdExhausted,
}

#[derive(Debug)]
pub struct Simulation {
    pub elapsed_seconds: u64,
    pub network: Network,
    trains: Vec<TrainEntity>,
    dwell_policy: DwellPolicy,
    next_train_id: usize,
    constraints: Vec<ConstraintRecord>,
    next_constraint_id: u64,
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
        let mut simulation = Self {
            elapsed_seconds: 0,
            network,
            trains: Vec::new(),
            dwell_policy,
            next_train_id: 0,
            constraints: Vec::new(),
            next_constraint_id: 0,
        };

        for train in trains {
            simulation
                .add_train(train)
                .expect("invalid initial train registration");
        }

        simulation
    }

    /// Registers a train and returns its stable, monotonically allocated identity.
    ///
    /// Registration fails without modifying the simulation if the train would
    /// conflict with existing resource ownership or the identity space is exhausted.
    pub fn add_train(&mut self, train: Train) -> Result<TrainId, AddTrainError> {
        let id = TrainId(self.next_train_id);

        let next_train_id = self
            .next_train_id
            .checked_add(1)
            .ok_or(AddTrainError::IdExhausted)?;

        let train_entity = TrainEntity::new(id, train);

        // Validate the prospective world before committing it.
        ResourceView::try_derive(
            self.trains
                .iter()
                .map(|entity| (entity.id(), entity.train()))
                .chain(std::iter::once((train_entity.id(), train_entity.train()))),
        )
        .map_err(|_| AddTrainError::ResourceConflict)?;

        // Commit only after validation succeeds.
        self.trains.push(train_entity);
        self.next_train_id = next_train_id;

        Ok(id)
    }

    /// Registers a constraint over `[start_at, end_at)`; any failure leaves state untouched.
    pub fn create_constraint_at(
        &mut self,
        constraint: OperationalConstraint,
        start_at: u64,
        end_at: Option<u64>,
        origin: ConstraintOrigin,
    ) -> Result<ConstraintId, ConstraintError> {
        if start_at <= self.elapsed_seconds {
            return Err(ConstraintError::InvalidStart);
        }
        if end_at.is_some_and(|end| end <= start_at) {
            return Err(ConstraintError::InvalidEnd);
        }
        self.validate_constraint_target(constraint)?;
        let next_constraint_id = self
            .next_constraint_id
            .checked_add(1)
            .ok_or(ConstraintError::IdExhausted)?;

        let id = ConstraintId(self.next_constraint_id);
        self.next_constraint_id = next_constraint_id;
        self.constraints.push(ConstraintRecord {
            id,
            constraint,
            start_at,
            end_at,
            origin,
        });
        Ok(id)
    }

    /// Normalizes relative timing; duration runs from activation, not registration.
    pub fn create_constraint_in(
        &mut self,
        constraint: OperationalConstraint,
        starts_in: u64,
        duration: Option<u64>,
        origin: ConstraintOrigin,
    ) -> Result<ConstraintId, ConstraintError> {
        if starts_in == 0 {
            return Err(ConstraintError::InvalidStart);
        }
        if duration == Some(0) {
            return Err(ConstraintError::InvalidEnd);
        }

        let now = self.elapsed_seconds;
        let start_at = now
            .checked_add(starts_in)
            .ok_or(ConstraintError::TimeOverflow)?;
        let end_at = duration
            .map(|duration| {
                start_at
                    .checked_add(duration)
                    .ok_or(ConstraintError::TimeOverflow)
            })
            .transpose()?;

        self.create_constraint_at(constraint, start_at, end_at, origin)
    }

    /// Removes a scheduled or active constraint immediately.
    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), ConstraintError> {
        let index = self
            .constraints
            .iter()
            .position(|record| record.id == id)
            .ok_or(ConstraintError::UnknownConstraint)?;
        self.constraints.remove(index);
        Ok(())
    }

    fn validate_constraint_target(
        &self,
        constraint: OperationalConstraint,
    ) -> Result<(), ConstraintError> {
        let known = |station: StationId| station.0 < self.network.station_count();
        match constraint {
            OperationalConstraint::StationDeparturesBlocked { station }
            | OperationalConstraint::StationUnavailable { station } => known(station)
                .then_some(())
                .ok_or(ConstraintError::UnknownStation),
            OperationalConstraint::TrackUnavailable { from, to } => {
                if !known(from) || !known(to) {
                    return Err(ConstraintError::UnknownStation);
                }
                self.network
                    .track(from, to)
                    .map(|_| ())
                    .ok_or(ConstraintError::UnknownTrack)
            }
        }
    }

    /// Operational values active at the current time; the only input for admission views.
    pub(crate) fn active_constraint_view(&self) -> ConstraintView {
        ConstraintView::from_constraints(
            self.constraints
                .iter()
                .filter(|record| record.is_active(self.elapsed_seconds))
                .map(|record| record.constraint),
        )
    }

    /// Applies a command without advancing time. Rejections leave state unchanged.
    pub fn apply_command(&mut self, command: TrainCommand) -> Result<(), CommandError> {
        match command {
            TrainCommand::Accelerate { train_id } => {
                let index = self
                    .trains
                    .iter()
                    .position(|train_entity| train_entity.id() == train_id)
                    .ok_or(CommandError::UnknownTrain)?;
                let train = &self.trains[index].train();

                if !train.is_manual_control() {
                    return Err(CommandError::NotManual);
                }

                let station = match train.state() {
                    TrainState::Moving { .. } => return Ok(()),
                    TrainState::AtStation { station, .. } => station,
                };
                let candidate =
                    select_departure_candidate(&self.network, station, train.direction())
                        .ok_or(CommandError::NoOutgoingTrack)?;
                let resources = ResourceView::derive(self.trains());
                let constraints = self.active_constraint_view();
                if !can_admit_departure(&candidate, &resources, &constraints) {
                    return Err(CommandError::Blocked);
                }
                self.trains[index].train_mut().apply_departure(
                    candidate.from,
                    candidate.to,
                    candidate.direction,
                );
                Ok(())
            }
        }
    }

    /// Returns an owned observation of the current state in train vector order and constraint ID order.
    pub fn snapshot(&self) -> SimulationSnapshot {
        let mut constraints: Vec<_> = self
            .constraints
            .iter()
            .filter(|record| !record.is_expired(self.elapsed_seconds))
            .map(|record| ConstraintSnapshot {
                id: record.id,
                constraint: record.constraint,
                start_at: record.start_at,
                end_at: record.end_at,
                origin: record.origin,
            })
            .collect();
        constraints.sort_unstable_by_key(|constraint| constraint.id.0);

        SimulationSnapshot {
            elapsed_seconds: self.elapsed_seconds,
            trains: self
                .trains
                .iter()
                .map(|train_entity| self.snapshot_train(train_entity))
                .collect(),
            constraints,
        }
    }

    fn snapshot_train(&self, train_entity: &TrainEntity) -> TrainSnapshot {
        let train = train_entity.train();
        TrainSnapshot {
            id: train_entity.id(),
            direction: train.direction(),
            velocity: train.velocity(),
            state: self.snapshot_train_state(&train.state()),
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
                train.advance_dwell(station, elapsed_seconds, dwell_seconds);
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
            train.arrive_at(to, dwell_seconds);
        } else {
            train.advance_movement(from, to, elapsed_seconds);
        }
    }

    pub fn step(&mut self) {
        // World N stays immutable until every automatic departure is resolved.
        let starting_resources = ResourceView::derive(self.trains());
        let constraints = self.active_constraint_view();
        let mut proposals = Vec::new();
        for (train_index, train_entity) in self.trains.iter().enumerate() {
            if !train_entity.train().is_automatic_control() {
                continue;
            }
            let TrainState::AtStation { station, state } = train_entity.train().state() else {
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
                    train_id: train_entity.id(),
                    candidate: select_departure_candidate(
                        &self.network,
                        station,
                        train_entity.train().direction(),
                    )
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
                if can_admit_departure(&candidate, &starting_resources, &constraints)
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
        for (train_entity, decision) in self.trains.iter_mut().zip(decisions) {
            let train = &mut train_entity.train_mut();
            match decision {
                AutomaticDepartureDecision::Accepted(candidate) => {
                    train.apply_departure(candidate.from, candidate.to, candidate.direction);
                    continue;
                }
                AutomaticDepartureDecision::Rejected { station } => {
                    train.reject_departure(station);
                    continue;
                }
                AutomaticDepartureDecision::NotEligible => {}
            }
            match train.state() {
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
        let now = self.elapsed_seconds;
        self.constraints.retain(|record| !record.is_expired(now));
    }

    pub fn train(&self, id: TrainId) -> &Train {
        self.trains
            .iter()
            .find(|entity| entity.id() == id)
            .map(|entity| entity.train())
            .unwrap()
    }

    pub fn trains(&self) -> impl Iterator<Item = (TrainId, &Train)> + '_ {
        self.trains
            .iter()
            .map(|entity| (entity.id(), entity.train()))
    }

    pub fn snapshot_train_by_id(
        snapshot: &SimulationSnapshot,
        train_id: TrainId,
    ) -> &TrainSnapshot {
        snapshot
            .trains
            .iter()
            .find(|train| train.id == train_id)
            .unwrap()
    }
}

#[cfg(test)]
impl Simulation {
    pub(crate) fn reverse_train_order_for_test(&mut self) {
        self.trains.reverse();
    }
}
