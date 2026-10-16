use crate::EmergencyReason;
use crate::frame::{Frame, valid_command_id};
use crate::identity::{CommandId, NodeId, StationId, TrainId};
use crate::message::{
    Command, CommandKind, CommandResult, Event, Location, LocationKind, Message, MessageKind,
    RejectionReason, StopReason,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::encode;
    use crate::message::{EmergencyReason, StopReason};

    const DEPART: [u8; 16] = [
        0x01, 0x01, 0x01, 0x00, 0x02, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x10, 0x00, 0x20,
        0x00,
    ];

    const EMERGENCY: [u8; 13] = [
        0x01, 0x02, 0x01, 0x00, 0x02, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01,
    ];

    const ACCEPTED_DEPART: [u8; 13] = [
        0x01, 0x10, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01,
    ];

    const ACCEPTED_EMERGENCY: [u8; 13] = [
        0x01, 0x10, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x02,
    ];

    const REJECTED_DEPART: [u8; 14] = [
        0x01, 0x11, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01, 0x02,
    ];

    const REJECTED_EMERGENCY: [u8; 14] = [
        0x01, 0x11, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x02, 0x06,
    ];

    const DEPARTED: [u8; 16] = [
        0x01, 0x20, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x10, 0x00, 0x20,
        0x00,
    ];

    const STOPPED_EMERGENCY: [u8; 18] = [
        0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01, 0x02, 0x10,
        0x00, 0x20, 0x00,
    ];

    const STOPPED_OBSTACLE: [u8; 18] = [
        0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x02, 0x02, 0x10,
        0x00, 0x20, 0x00,
    ];

    const STOPPED_STATION: [u8; 18] = [
        0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x03, 0x01, 0x10,
        0x00, 0x00, 0x00,
    ];

    const STOPPED_UNKNOWN: [u8; 18] = [
        0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ];

    fn node(n: u16) -> NodeId {
        NodeId::new(n).unwrap()
    }

    fn train(n: u16) -> TrainId {
        TrainId::new(n).unwrap()
    }

    fn station(n: u16) -> StationId {
        StationId::new(n).unwrap()
    }

    fn command(n: u32) -> Option<CommandId> {
        Some(CommandId::new(n).unwrap())
    }

    fn expected_frame(kind: Message, command_id: Option<CommandId>, reply: bool) -> Frame {
        Frame {
            source: node(if reply { 2 } else { 1 }),
            destination: node(if reply { 1 } else { 2 }),
            command_id,
            message: kind,
        }
    }

    fn segment() -> Location {
        Location::OnSegment {
            from: station(16),
            to: station(32),
        }
    }

    fn assert_invalid_value(input: &[u8], field: Field) {
        assert_eq!(decode(input), Err(DecodeError::InvalidValue { field }),);
    }

    fn assert_invalid_tag(input: &[u8], field: Field, actual: u8) {
        assert_eq!(
            decode(input),
            Err(DecodeError::InvalidDiscriminant { field, actual }),
        );
    }

    // --------------------------------------------------
    // 1. Official SPEC-001 binary vectors
    // --------------------------------------------------

    #[test]
    fn decode_depart_matches_spec() {
        let expected = expected_frame(
            Message::Command(Command::Depart {
                train_id: train(3),
                from: station(16),
                to: station(32),
            }),
            command(0x01020304),
            false,
        );

        assert_eq!(decode(&DEPART), Ok(expected));
    }

    #[test]
    fn decode_emergency_stop_matches_spec() {
        let expected = expected_frame(
            Message::Command(Command::EmergencyStop {
                train_id: train(3),
                reason: EmergencyReason::OperatorRequest,
            }),
            command(0x01020305),
            false,
        );

        assert_eq!(decode(&EMERGENCY), Ok(expected));
    }

    #[test]
    fn decode_accepted_depart_matches_spec() {
        let expected = expected_frame(
            Message::CommandResult(CommandResult::Accepted {
                train_id: train(3),
                command_kind: CommandKind::Depart,
            }),
            command(0x01020304),
            true,
        );

        assert_eq!(decode(&ACCEPTED_DEPART), Ok(expected));
    }

    #[test]
    fn decode_accepted_emergency_matches_spec() {
        let expected = expected_frame(
            Message::CommandResult(CommandResult::Accepted {
                train_id: train(3),
                command_kind: CommandKind::EmergencyStop,
            }),
            command(0x01020305),
            true,
        );

        assert_eq!(decode(&ACCEPTED_EMERGENCY), Ok(expected));
    }

    #[test]
    fn decode_rejected_depart_matches_spec() {
        let expected = expected_frame(
            Message::CommandResult(CommandResult::Rejected {
                train_id: train(3),
                command_kind: CommandKind::Depart,
                reason: RejectionReason::LocationMismatch,
            }),
            command(0x01020304),
            true,
        );

        assert_eq!(decode(&REJECTED_DEPART), Ok(expected));
    }

    #[test]
    fn decode_rejected_emergency_matches_spec() {
        let expected = expected_frame(
            Message::CommandResult(CommandResult::Rejected {
                train_id: train(3),
                command_kind: CommandKind::EmergencyStop,
                reason: RejectionReason::UnableToExecute,
            }),
            command(0x01020305),
            true,
        );

        assert_eq!(decode(&REJECTED_EMERGENCY), Ok(expected));
    }

    #[test]
    fn decode_departed_matches_spec() {
        let expected = expected_frame(
            Message::Event(Event::Departed {
                train_id: train(3),
                from: station(16),
                to: station(32),
            }),
            command(0x01020304),
            true,
        );

        assert_eq!(decode(&DEPARTED), Ok(expected));
    }

    #[test]
    fn decode_stopped_emergency_matches_spec() {
        let expected = expected_frame(
            Message::Event(Event::Stopped {
                train_id: train(3),
                reason: StopReason::EmergencyCommand,
                location: segment(),
            }),
            command(0x01020305),
            true,
        );

        assert_eq!(decode(&STOPPED_EMERGENCY), Ok(expected));
    }

    #[test]
    fn decode_stopped_obstacle_matches_spec() {
        let expected = expected_frame(
            Message::Event(Event::Stopped {
                train_id: train(3),
                reason: StopReason::Obstacle,
                location: segment(),
            }),
            None,
            true,
        );

        assert_eq!(decode(&STOPPED_OBSTACLE), Ok(expected));
    }

    #[test]
    fn decode_stopped_station_matches_spec() {
        let expected = expected_frame(
            Message::Event(Event::Stopped {
                train_id: train(3),
                reason: StopReason::LocalStop,
                location: Location::AtStation(station(16)),
            }),
            None,
            true,
        );

        assert_eq!(decode(&STOPPED_STATION), Ok(expected));
    }

    #[test]
    fn decode_stopped_unknown_matches_spec() {
        let expected = expected_frame(
            Message::Event(Event::Stopped {
                train_id: train(3),
                reason: StopReason::Unspecified,
                location: Location::Unknown,
            }),
            None,
            true,
        );

        assert_eq!(decode(&STOPPED_UNKNOWN), Ok(expected));
    }

    // --------------------------------------------------
    // 2. Header and exact length validation
    // --------------------------------------------------

    #[test]
    fn rejects_every_short_header() {
        for len in 0..10 {
            assert_eq!(
                decode(&DEPART[..len]),
                Err(DecodeError::HeaderTooShort { actual: len }),
            );
        }
    }

    #[test]
    fn rejects_oversized_frame() {
        let input = [0u8; 33];

        assert_eq!(
            decode(&input),
            Err(DecodeError::FrameTooLong { actual: 33 }),
        );
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut input = DEPART;
        input[0] = 0x02;

        assert_eq!(
            decode(&input),
            Err(DecodeError::UnsupportedVersion { actual: 0x02 }),
        );
    }

    #[test]
    fn rejects_unknown_message_kind() {
        let mut input = DEPART;
        input[1] = 0xFF;

        assert_eq!(
            decode(&input),
            Err(DecodeError::UnknownMessageKind { actual: 0xFF }),
        );
    }

    #[test]
    fn rejects_truncated_frames_for_all_kinds() {
        let vectors: &[&[u8]] = &[
            &DEPART,
            &EMERGENCY,
            &ACCEPTED_DEPART,
            &REJECTED_DEPART,
            &DEPARTED,
            &STOPPED_OBSTACLE,
        ];

        for vector in vectors {
            for len in 10..vector.len() {
                assert_eq!(
                    decode(&vector[..len]),
                    Err(DecodeError::InvalidLength {
                        expected: vector.len(),
                        actual: len,
                    }),
                );
            }
        }
    }

    #[test]
    fn rejects_appended_bytes_for_all_kinds() {
        let vectors: &[&[u8]] = &[
            &DEPART,
            &EMERGENCY,
            &ACCEPTED_DEPART,
            &REJECTED_DEPART,
            &DEPARTED,
            &STOPPED_OBSTACLE,
        ];

        for vector in vectors {
            let mut input = [0u8; 32];
            input[..vector.len()].copy_from_slice(vector);

            let actual = vector.len() + 1;

            assert_eq!(
                decode(&input[..actual]),
                Err(DecodeError::InvalidLength {
                    expected: vector.len(),
                    actual,
                }),
            );

            assert_eq!(
                decode(&input),
                Err(DecodeError::InvalidLength {
                    expected: vector.len(),
                    actual: 32,
                }),
            );
        }
    }

    // --------------------------------------------------
    // 3. Identity and correlation validation
    // --------------------------------------------------

    #[test]
    fn rejects_zero_source_node() {
        let mut input = DEPART;
        input[2..4].fill(0);

        assert_invalid_value(&input, Field::SourceNodeId);
    }

    #[test]
    fn rejects_zero_destination_node() {
        let mut input = DEPART;
        input[4..6].fill(0);

        assert_invalid_value(&input, Field::DestinationNodeId);
    }

    #[test]
    fn rejects_equal_source_and_destination() {
        let mut input = DEPART;
        input[4..6].copy_from_slice(&[1, 0]);

        assert_invalid_value(&input, Field::DestinationNodeId);
    }

    #[test]
    fn rejects_zero_train_id_for_every_message_kind() {
        let vectors: &[&[u8]] = &[
            &DEPART,
            &EMERGENCY,
            &ACCEPTED_DEPART,
            &REJECTED_DEPART,
            &DEPARTED,
            &STOPPED_OBSTACLE,
        ];

        for vector in vectors {
            let mut input = [0u8; 32];
            input[..vector.len()].copy_from_slice(vector);
            input[10..12].fill(0);

            assert_invalid_value(&input[..vector.len()], Field::TrainId);
        }
    }

    #[test]
    fn rejects_missing_command_id_for_correlated_messages() {
        let vectors: &[&[u8]] = &[
            &DEPART,
            &EMERGENCY,
            &ACCEPTED_DEPART,
            &REJECTED_DEPART,
            &DEPARTED,
        ];

        for vector in vectors {
            let mut input = [0u8; 32];
            input[..vector.len()].copy_from_slice(vector);
            input[6..10].fill(0);

            assert_invalid_value(&input[..vector.len()], Field::CommandId);
        }
    }

    #[test]
    fn rejects_invalid_departure_endpoints() {
        let mut input = DEPART;
        input[12..14].fill(0);
        assert_invalid_value(&input, Field::FromStationId);

        let mut input = DEPART;
        input[14..16].fill(0);
        assert_invalid_value(&input, Field::ToStationId);

        let mut input = DEPART;
        input[14..16].copy_from_slice(&[0x10, 0x00]);
        assert_invalid_value(&input, Field::ToStationId);
    }

    #[test]
    fn rejects_invalid_departed_endpoints() {
        let mut input = DEPARTED;
        input[12..14].fill(0);
        assert_invalid_value(&input, Field::FromStationId);

        let mut input = DEPARTED;
        input[14..16].fill(0);
        assert_invalid_value(&input, Field::ToStationId);

        let mut input = DEPARTED;
        input[14..16].copy_from_slice(&[0x10, 0x00]);
        assert_invalid_value(&input, Field::ToStationId);
    }

    // --------------------------------------------------
    // 4. Invalid discriminants and combinations
    // --------------------------------------------------

    #[test]
    fn rejects_invalid_emergency_reason() {
        let mut input = EMERGENCY;
        input[12] = 0xFF;

        assert_invalid_tag(&input, Field::EmergencyReason, 0xFF);
    }

    #[test]
    fn rejects_invalid_accepted_command_kind() {
        let mut input = ACCEPTED_DEPART;
        input[12] = 0xFF;

        assert_invalid_tag(&input, Field::CommandKind, 0xFF);
    }

    #[test]
    fn rejects_invalid_rejected_command_kind() {
        let mut input = REJECTED_DEPART;
        input[12] = 0xFF;

        assert_invalid_tag(&input, Field::CommandKind, 0xFF);
    }

    #[test]
    fn rejects_invalid_rejection_reason() {
        let mut input = REJECTED_DEPART;
        input[13] = 0xFF;

        assert_invalid_tag(&input, Field::RejectionReason, 0xFF);
    }

    #[test]
    fn rejects_emergency_with_invalid_rejection_combination() {
        let mut input = REJECTED_EMERGENCY;
        input[13] = 0x02; // LocationMismatch

        assert_invalid_value(&input, Field::Reason);
    }

    #[test]
    fn rejects_invalid_stop_reason() {
        let mut input = STOPPED_OBSTACLE;
        input[12] = 0xFF;

        assert_invalid_tag(&input, Field::StopReason, 0xFF);
    }

    #[test]
    fn rejects_invalid_location_kind() {
        let mut input = STOPPED_OBSTACLE;
        input[13] = 0xFF;

        assert_invalid_tag(&input, Field::LocationKind, 0xFF);
    }

    // --------------------------------------------------
    // 5. Stopped command ID and location semantics
    // --------------------------------------------------

    #[test]
    fn rejects_emergency_stop_event_without_command_id() {
        let mut input = STOPPED_EMERGENCY;
        input[6..10].fill(0);

        assert_invalid_value(&input, Field::CommandId);
    }

    #[test]
    fn rejects_unsolicited_stops_with_command_id() {
        for reason in [0x02, 0x03, 0x04] {
            let mut input = STOPPED_UNKNOWN;
            input[12] = reason;
            input[6..10].copy_from_slice(&0x01020304u32.to_le_bytes());

            assert_invalid_value(&input, Field::CommandId);
        }
    }

    #[test]
    fn rejects_unknown_location_with_nonzero_padding() {
        for index in 14..18 {
            let mut input = STOPPED_UNKNOWN;
            input[index] = 1;

            let field = if index < 16 {
                Field::LocationA
            } else {
                Field::LocationB
            };

            assert_invalid_value(&input, field);
        }
    }

    #[test]
    fn rejects_at_station_with_invalid_location_fields() {
        let mut input = STOPPED_STATION;
        input[14..16].fill(0);
        assert_invalid_value(&input, Field::LocationA);

        let mut input = STOPPED_STATION;
        input[16] = 1;
        assert_invalid_value(&input, Field::LocationB);
    }

    #[test]
    fn rejects_segment_with_invalid_location_fields() {
        let mut input = STOPPED_OBSTACLE;
        input[14..16].fill(0);
        assert_invalid_value(&input, Field::LocationA);

        let mut input = STOPPED_OBSTACLE;
        input[16..18].fill(0);
        assert_invalid_value(&input, Field::LocationB);

        let mut input = STOPPED_OBSTACLE;
        input[16..18].copy_from_slice(&[0x10, 0x00]);
        assert_invalid_value(&input, Field::LocationB);
    }

    // --------------------------------------------------
    // 6. Error precedence
    // --------------------------------------------------

    #[test]
    fn too_long_has_priority_over_invalid_version() {
        let input = [0xFFu8; 33];

        assert_eq!(
            decode(&input),
            Err(DecodeError::FrameTooLong { actual: 33 }),
        );
    }

    #[test]
    fn unsupported_version_has_priority_over_unknown_kind() {
        let mut input = DEPART;
        input[0] = 0xFF;
        input[1] = 0xFF;

        assert_eq!(
            decode(&input),
            Err(DecodeError::UnsupportedVersion { actual: 0xFF }),
        );
    }

    #[test]
    fn invalid_length_has_priority_over_invalid_node() {
        let mut input = DEPART;
        input[2..4].fill(0);

        assert_eq!(
            decode(&input[..15]),
            Err(DecodeError::InvalidLength {
                expected: 16,
                actual: 15,
            }),
        );
    }

    #[test]
    fn command_id_is_validated_before_train_id() {
        let mut input = DEPART;
        input[6..10].fill(0);
        input[10..12].fill(0);

        assert_invalid_value(&input, Field::CommandId);
    }

    #[test]
    fn train_id_is_validated_before_stop_reason() {
        let mut input = STOPPED_UNKNOWN;
        input[10..12].fill(0);
        input[12] = 0xFF;

        assert_invalid_value(&input, Field::TrainId);
    }

    #[test]
    fn unknown_stop_reason_has_priority_over_command_id_rule() {
        let mut input = STOPPED_EMERGENCY;
        input[6..10].fill(0);
        input[12] = 0xFF;

        assert_invalid_tag(&input, Field::StopReason, 0xFF);
    }

    #[test]
    fn stopped_location_kind_has_priority_over_command_id_rule() {
        let mut input = STOPPED_EMERGENCY;
        input[6..10].fill(0);
        input[13] = 0xFF;

        assert_invalid_tag(&input, Field::LocationKind, 0xFF);
    }

    #[test]
    fn stopped_command_id_rule_has_priority_over_invalid_location_fields() {
        let mut input = STOPPED_EMERGENCY;
        input[6..10].fill(0);
        input[14..16].fill(0);

        assert_invalid_value(&input, Field::CommandId);
    }

    // --------------------------------------------------
    // 7. Round trips and robustness
    // --------------------------------------------------

    #[test]
    fn official_vectors_round_trip_exactly() {
        let vectors: &[&[u8]] = &[
            &DEPART,
            &EMERGENCY,
            &ACCEPTED_DEPART,
            &ACCEPTED_EMERGENCY,
            &REJECTED_DEPART,
            &REJECTED_EMERGENCY,
            &DEPARTED,
            &STOPPED_EMERGENCY,
            &STOPPED_OBSTACLE,
            &STOPPED_STATION,
            &STOPPED_UNKNOWN,
        ];

        for bytes in vectors {
            let frame = decode(bytes).unwrap();

            let mut output = [0xAAu8; 32];
            let written = encode(&frame, &mut output).unwrap();

            assert_eq!(written, bytes.len());
            assert_eq!(&output[..written], *bytes);
        }
    }

    #[test]
    fn decode_is_stateless() {
        let first = decode(&DEPART).unwrap();
        let second = decode(&DEPART).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn accepts_maximum_valid_id_values() {
        let mut input = DEPART;

        input[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
        input[4..6].copy_from_slice(&1u16.to_le_bytes());
        input[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        input[10..12].copy_from_slice(&u16::MAX.to_le_bytes());
        input[12..14].copy_from_slice(&u16::MAX.to_le_bytes());
        input[14..16].copy_from_slice(&1u16.to_le_bytes());

        let frame = decode(&input).unwrap();

        assert_eq!(frame.source.get(), u16::MAX);
        assert_eq!(frame.command_id.unwrap().get(), u32::MAX);

        assert_eq!(
            frame.message,
            Message::Command(Command::Depart {
                train_id: train(u16::MAX),
                from: station(u16::MAX),
                to: station(1),
            })
        );
    }

    #[test]
    fn node_and_train_ids_are_independent() {
        let mut input = DEPART;

        input[2..4].copy_from_slice(&3u16.to_le_bytes());
        input[4..6].copy_from_slice(&4u16.to_le_bytes());

        let frame = decode(&input).unwrap();

        assert_eq!(frame.source, node(3));

        assert!(matches!(
            frame.message,
            Message::Command(Command::Depart {
                train_id,
                ..
            }) if train_id == train(3)
        ));
    }

    #[test]
    fn arbitrary_inputs_never_panic() {
        for len in 0..=33 {
            for seed in 0u8..=255 {
                let mut input = [0u8; 33];

                for (i, byte) in input[..len].iter_mut().enumerate() {
                    *byte = seed.wrapping_add(i as u8);
                }

                let _ = decode(&input[..len]);
            }
        }
    }
}
