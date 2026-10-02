use crate::station::StationId;
use crate::train::{Direction, Train, TrainId, TrainState};

pub fn moving_train(id: usize, from: StationId, to: StationId, direction: Direction) -> Train {
    let mut train = Train::new(TrainId(id), 100, from, direction);
    train.state = TrainState::Moving {
        from,
        to,
        elapsed_seconds: 0,
    };
    train.velocity = 1;
    train
}
