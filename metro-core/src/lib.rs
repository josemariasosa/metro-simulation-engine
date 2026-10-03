pub mod command;
mod domain;
pub mod simulation;
pub mod snapshot;

#[cfg(test)]
pub mod test_utils;

pub use domain::dwell::DwellPolicy;
pub use domain::network::Network;
pub use domain::station::StationId;
pub use domain::track::Track;
pub use domain::train::{AtStationState, Direction, Train, TrainControl, TrainState};
