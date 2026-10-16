#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId(u16);

impl NodeId {
    pub fn new(value: u16) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub fn get(&self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainId(u16);

impl TrainId {
    pub fn new(value: u16) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub fn get(&self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StationId(u16);

impl StationId {
    pub fn new(value: u16) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub fn get(&self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandId(u32);

impl CommandId {
    pub fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub fn get(&self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id() {
        assert!(NodeId::new(0).is_none());
        let brain = NodeId::new(1).unwrap();
        let train = NodeId::new(2).unwrap();
        assert_eq!(brain.get(), 1);
        assert_eq!(train.get(), 2);
        assert_ne!(brain, train);

        assert_eq!(NodeId::new(u16::MAX).unwrap().get(), u16::MAX);
    }

    #[test]
    fn test_train_id() {
        assert!(TrainId::new(0).is_none());
        let train_1 = TrainId::new(1).unwrap();
        let train_2 = TrainId::new(2).unwrap();
        assert_eq!(train_1.get(), 1);
        assert_eq!(train_2.get(), 2);
        assert_ne!(train_1, train_2);

        assert_eq!(TrainId::new(u16::MAX).unwrap().get(), u16::MAX);
    }

    #[test]
    fn test_station_id() {
        assert!(StationId::new(0).is_none());
        let station_1 = StationId::new(1).unwrap();
        let station_2 = StationId::new(2).unwrap();
        assert_eq!(station_1.get(), 1);
        assert_eq!(station_2.get(), 2);
        assert_ne!(station_1, station_2);

        assert_eq!(StationId::new(u16::MAX).unwrap().get(), u16::MAX);
    }

    #[test]
    fn test_command_id() {
        assert!(CommandId::new(0).is_none());
        let command_1 = CommandId::new(1).unwrap();
        let command_2 = CommandId::new(2).unwrap();
        assert_eq!(command_1.get(), 1);
        assert_eq!(command_2.get(), 2);
        assert_ne!(command_1, command_2);

        assert_eq!(CommandId::new(u32::MAX).unwrap().get(), u32::MAX);
    }
}
