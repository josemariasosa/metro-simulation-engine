use crate::{Train, TrainId};

/// A train registered in a simulation, paired with its stable simulation identity.
#[derive(Debug, Clone)]
pub(crate) struct TrainEntity {
    id: TrainId,
    train: Train,
}

impl TrainEntity {
    pub(crate) fn new(id: TrainId, train: Train) -> Self {
        Self { id, train }
    }

    pub fn id(&self) -> TrainId {
        self.id
    }

    pub fn train(&self) -> &Train {
        &self.train
    }

    pub fn train_mut(&mut self) -> &mut Train {
        &mut self.train
    }
}
