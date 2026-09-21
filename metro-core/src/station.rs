#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StationId(pub usize);

#[derive(Debug)]
pub struct Station {
    pub id: StationId,
    pub name: String,
}
