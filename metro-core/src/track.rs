use crate::station::StationId;

#[derive(Debug)]
pub struct Track {
    pub from: StationId,
    pub to: StationId,
    pub travel_seconds: u64,
}
