use crate::frame::{Frame, valid_command_id};
use crate::identity::{CommandId, NodeId, StationId, TrainId};
use crate::message::{
    Command, CommandKind, CommandResult, EmergencyReason, Event, Location, LocationKind, Message,
    MessageKind, RejectionReason, StopReason,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    SourceNodeId,
    DestinationNodeId,
    TrainId,
    CommandId,
    FromStationId,
    ToStationId,
    CommandKind,
    EmergencyReason,
    RejectionReason,
    StopReason,
    LocationKind,
    LocationA,
    LocationB,
    Reason,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    FrameTooLong { actual: usize },
    HeaderTooShort { actual: usize },
    UnsupportedVersion { actual: u8 },
    UnknownMessageKind { actual: u8 },
    InvalidLength { expected: usize, actual: usize },
    InvalidValue { field: Field },
    InvalidDiscriminant { field: Field, actual: u8 },
}

pub fn decode(input: &[u8]) -> Result<Frame, DecodeError> {
    check_input_length(input)?;
    check_input_version(input)?;

    let message_kind = MessageKind::try_from(input[1])?;
    check_message_kind_length(input, &message_kind)?;

    let source = parse_node_id(&input[2..4], Field::SourceNodeId)?;
    let destination = parse_node_id(&input[4..6], Field::DestinationNodeId)?;
    if source == destination {
        return Err(DecodeError::InvalidValue {
            field: Field::DestinationNodeId,
        });
    }

    let command_id = parse_command_id(&input[6..10]);

    // Preserve header precedence by rejecting missing IDs before parsing non-Stopped bodies.
    if message_kind != MessageKind::Stopped && command_id.is_none() {
        return Err(DecodeError::InvalidValue {
            field: Field::CommandId,
        });
    }

    let message = parse_message(input, &message_kind, command_id)?;

    if message_kind != MessageKind::Stopped && !valid_command_id(&message, command_id) {
        return Err(DecodeError::InvalidValue {
            field: Field::CommandId,
        });
    }

    Ok(Frame {
        source,
        destination,
        command_id,
        message,
    })
}

fn check_input_length(input: &[u8]) -> Result<(), DecodeError> {
    if input.len() > crate::constants::MAX_FRAME_SIZE {
        return Err(DecodeError::FrameTooLong {
            actual: input.len(),
        });
    }
    if input.len() < crate::constants::MIN_FRAME_SIZE {
        return Err(DecodeError::HeaderTooShort {
            actual: input.len(),
        });
    }
    Ok(())
}

fn check_input_version(input: &[u8]) -> Result<(), DecodeError> {
    if input[0] != crate::constants::PROTOCOL_VERSION {
        return Err(DecodeError::UnsupportedVersion { actual: input[0] });
    }
    Ok(())
}

fn check_message_kind_length(input: &[u8], message_kind: &MessageKind) -> Result<(), DecodeError> {
    if input.len() != message_kind.encoded_len() {
        return Err(DecodeError::InvalidLength {
            expected: message_kind.encoded_len(),
            actual: input.len(),
        });
    }
    Ok(())
}

fn parse_node_id(input: &[u8], field: Field) -> Result<NodeId, DecodeError> {
    NodeId::from_le_bytes(input.try_into().unwrap()).ok_or(DecodeError::InvalidValue { field })
}

fn parse_train_id(input: &[u8]) -> Result<TrainId, DecodeError> {
    TrainId::from_le_bytes(input.try_into().unwrap()).ok_or(DecodeError::InvalidValue {
        field: Field::TrainId,
    })
}

fn parse_station_id(input: &[u8], field: Field) -> Result<StationId, DecodeError> {
    StationId::from_le_bytes(input.try_into().unwrap()).ok_or(DecodeError::InvalidValue { field })
}

fn parse_command_id(input: &[u8]) -> Option<CommandId> {
    CommandId::from_le_bytes(input.try_into().unwrap())
}

fn parse_message(
    input: &[u8],
    message_kind: &MessageKind,
    command_id: Option<CommandId>,
) -> Result<Message, DecodeError> {
    match message_kind {
        MessageKind::Depart => parse_message_depart(input),
        MessageKind::EmergencyStop => parse_message_emergency_stop(input),
        MessageKind::Accepted => parse_message_accepted(input),
        MessageKind::Rejected => parse_message_rejected(input),
        MessageKind::Departed => parse_message_departed(input),
        MessageKind::Stopped => parse_message_stopped(input, command_id),
    }
}

fn parse_message_depart(input: &[u8]) -> Result<Message, DecodeError> {
    let train_id = parse_train_id(&input[10..12])?;
    let from = parse_station_id(&input[12..14], Field::FromStationId)?;
    let to = parse_station_id(&input[14..16], Field::ToStationId)?;

    if from == to {
        return Err(DecodeError::InvalidValue {
            field: Field::ToStationId,
        });
    }

    Ok(Message::Command(Command::Depart { train_id, from, to }))
}

fn parse_message_emergency_stop(input: &[u8]) -> Result<Message, DecodeError> {
    let train_id = parse_train_id(&input[10..12])?;
    let reason = EmergencyReason::try_from(input[12])?;

    Ok(Message::Command(Command::EmergencyStop {
        train_id,
        reason,
    }))
}

fn parse_message_accepted(input: &[u8]) -> Result<Message, DecodeError> {
    let train_id = parse_train_id(&input[10..12])?;
    let command_kind = CommandKind::try_from(input[12])?;
    Ok(Message::CommandResult(CommandResult::Accepted {
        train_id,
        command_kind,
    }))
}

fn parse_message_rejected(input: &[u8]) -> Result<Message, DecodeError> {
    let train_id = parse_train_id(&input[10..12])?;
    let command_kind = CommandKind::try_from(input[12])?;
    let reason = RejectionReason::try_from(input[13])?;

    if command_kind == CommandKind::EmergencyStop && reason != RejectionReason::UnableToExecute {
        return Err(DecodeError::InvalidValue {
            field: Field::Reason,
        });
    }

    Ok(Message::CommandResult(CommandResult::Rejected {
        train_id,
        command_kind,
        reason,
    }))
}

fn parse_message_departed(input: &[u8]) -> Result<Message, DecodeError> {
    let train_id = parse_train_id(&input[10..12])?;
    let from = parse_station_id(&input[12..14], Field::FromStationId)?;
    let to = parse_station_id(&input[14..16], Field::ToStationId)?;

    if from == to {
        return Err(DecodeError::InvalidValue {
            field: Field::ToStationId,
        });
    }

    Ok(Message::Event(Event::Departed { train_id, from, to }))
}

fn parse_message_stopped(
    input: &[u8],
    command_id: Option<CommandId>,
) -> Result<Message, DecodeError> {
    let train_id = parse_train_id(&input[10..12])?;
    let reason = StopReason::try_from(input[12])?;
    let location_kind = LocationKind::try_from(input[13])?;

    // Check the cross-field rule after both discriminants and before location fields.
    let message = Message::Event(Event::Stopped {
        train_id,
        reason,
        location: Location::Unknown,
    });
    if !valid_command_id(&message, command_id) {
        return Err(DecodeError::InvalidValue {
            field: Field::CommandId,
        });
    }

    let location = match location_kind {
        LocationKind::Unknown => {
            check_all_zero(&input[14..16], Field::LocationA)?;
            check_all_zero(&input[16..18], Field::LocationB)?;
            Location::Unknown
        }
        LocationKind::AtStation => {
            let station_id = parse_station_id(&input[14..16], Field::LocationA)?;
            check_all_zero(&input[16..18], Field::LocationB)?;
            Location::AtStation(station_id)
        }
        LocationKind::OnSegment => {
            let from = parse_station_id(&input[14..16], Field::LocationA)?;
            let to = parse_station_id(&input[16..18], Field::LocationB)?;

            if from == to {
                return Err(DecodeError::InvalidValue {
                    field: Field::LocationB,
                });
            }

            Location::OnSegment { from, to }
        }
    };

    Ok(Message::Event(Event::Stopped {
        train_id,
        reason,
        location,
    }))
}

fn check_all_zero(input: &[u8], field: Field) -> Result<(), DecodeError> {
    if input.iter().all(|&byte| byte == 0) {
        Ok(())
    } else {
        Err(DecodeError::InvalidValue { field })
    }
}
