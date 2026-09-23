use crate::station::StationId;
use crate::train::Train;

#[derive(Debug, Default, Clone)]
pub struct DwellPolicy;

impl DwellPolicy {
    pub fn new() -> Self {
        Self
    }

    pub fn dwell_seconds(&self, _station: StationId, _train: &Train) -> u64 {
        3
    }
}
