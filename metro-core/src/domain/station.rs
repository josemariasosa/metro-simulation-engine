#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StationId(pub usize);

#[derive(Debug)]
pub(crate) struct Station {
    pub(crate) id: StationId,
    pub(crate) name: String,
}
