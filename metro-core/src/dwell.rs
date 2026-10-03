use crate::domain::train::Train;
use crate::station::StationId;

#[derive(Debug, Default, Clone)]
pub struct DwellPolicy;

impl DwellPolicy {
    pub fn new() -> Self {
        Self
    }

    pub fn dwell_seconds(&self, _station: StationId, _train: &Train) -> u64 {
        dwell_seconds_internal()
    }

    #[cfg(test)]
    pub fn default_dwell_seconds() -> u64 {
        dwell_seconds_internal()
    }
}

fn dwell_seconds_internal() -> u64 {
    3
}
