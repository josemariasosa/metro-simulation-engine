use crate::frame::Frame;
use crate::message::{
    Command, CommandKind, CommandResult, Event, Location, Message, MessageKind, RejectionReason,
    StopReason,
};

#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError {
    InvalidValue,
    BufferTooSmall { required: usize, available: usize },
}

pub fn encode(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    if frame.source == frame.destination {
        return Err(EncodeError::InvalidValue);
    }

    match frame.message.kind() {
        MessageKind::Depart => encode_depart(frame, output),
        MessageKind::EmergencyStop => encode_emergency_stop(frame, output),
        MessageKind::Accepted => encode_accepted(frame, output),
        MessageKind::Rejected => encode_rejected(frame, output),
        MessageKind::Departed => encode_departed(frame, output),
        MessageKind::Stopped => encode_stopped(frame, output),
    }
}

fn common_output_prefix(frame: &Frame, output: &mut [u8]) {
    output[0] = crate::constants::PROTOCOL_VERSION;
    output[1] = frame.message.kind() as u8;
    output[2..4].copy_from_slice(&frame.source.get().to_le_bytes());
    output[4..6].copy_from_slice(&frame.destination.get().to_le_bytes());
    output[6..10].copy_from_slice(&frame.command_id.map_or(0, |id| id.get()).to_le_bytes());
}

fn encode_depart(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    let required = frame.message.kind().encoded_len();

    if let Message::Command(Command::Depart { train_id, from, to }) = &frame.message {
        if frame.command_id.is_none() || from == to {
            return Err(EncodeError::InvalidValue);
        }

        if output.len() < required {
            return Err(EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            });
        }

        common_output_prefix(frame, output);
        output[10..12].copy_from_slice(&train_id.get().to_le_bytes());
        output[12..14].copy_from_slice(&from.get().to_le_bytes());
        output[14..16].copy_from_slice(&to.get().to_le_bytes());
    }

    Ok(required)
}

fn encode_emergency_stop(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    let required = frame.message.kind().encoded_len();

    if let Message::Command(Command::EmergencyStop { train_id, reason }) = &frame.message {
        if frame.command_id.is_none() {
            return Err(EncodeError::InvalidValue);
        }

        if output.len() < required {
            return Err(EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            });
        }

        common_output_prefix(frame, output);
        output[10..12].copy_from_slice(&train_id.get().to_le_bytes());
        output[12] = *reason as u8;
    }

    Ok(required)
}

fn encode_accepted(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    let required = frame.message.kind().encoded_len();

    if let Message::CommandResult(CommandResult::Accepted {
        train_id,
        command_kind,
    }) = &frame.message
    {
        if frame.command_id.is_none() {
            return Err(EncodeError::InvalidValue);
        }

        if output.len() < required {
            return Err(EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            });
        }

        common_output_prefix(frame, output);
        output[10..12].copy_from_slice(&train_id.get().to_le_bytes());
        output[12] = *command_kind as u8;
    }

    Ok(required)
}

fn encode_rejected(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    let required = frame.message.kind().encoded_len();

    if let Message::CommandResult(CommandResult::Rejected {
        train_id,
        command_kind,
        reason,
    }) = &frame.message
    {
        if frame.command_id.is_none()
            || (*command_kind == CommandKind::EmergencyStop
                && *reason != RejectionReason::UnableToExecute)
        {
            return Err(EncodeError::InvalidValue);
        }

        if output.len() < required {
            return Err(EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            });
        }

        common_output_prefix(frame, output);
        output[10..12].copy_from_slice(&train_id.get().to_le_bytes());
        output[12] = *command_kind as u8;
        output[13] = *reason as u8;
    }

    Ok(required)
}

fn encode_departed(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    let required = frame.message.kind().encoded_len();

    if let Message::Event(Event::Departed { train_id, from, to }) = &frame.message {
        if frame.command_id.is_none() || from == to {
            return Err(EncodeError::InvalidValue);
        }

        if output.len() < required {
            return Err(EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            });
        }

        common_output_prefix(frame, output);
        output[10..12].copy_from_slice(&train_id.get().to_le_bytes());
        output[12..14].copy_from_slice(&from.get().to_le_bytes());
        output[14..16].copy_from_slice(&to.get().to_le_bytes());
    }

    Ok(required)
}

fn encode_stopped(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    let required = frame.message.kind().encoded_len();

    if let Message::Event(Event::Stopped {
        train_id,
        reason,
        location,
    }) = &frame.message
    {
        if reason == &StopReason::EmergencyCommand {
            if frame.command_id.is_none() {
                return Err(EncodeError::InvalidValue);
            }
        } else {
            if frame.command_id.is_some() {
                return Err(EncodeError::InvalidValue);
            }
        }

        if let Location::OnSegment { from, to } = location {
            if from == to {
                return Err(EncodeError::InvalidValue);
            }
        }

        if output.len() < required {
            return Err(EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            });
        }

        common_output_prefix(frame, output);
        output[10..12].copy_from_slice(&train_id.get().to_le_bytes());
        output[12] = *reason as u8;
        output[13] = location.kind() as u8;
        match location {
            Location::Unknown => {
                output[14..16].copy_from_slice(&[0u8; 2]);
                output[16..18].copy_from_slice(&[0u8; 2]);
            }
            Location::AtStation(station_id) => {
                output[14..16].copy_from_slice(&station_id.get().to_le_bytes());
                output[16..18].copy_from_slice(&[0u8; 2]);
            }
            Location::OnSegment { from, to } => {
                output[14..16].copy_from_slice(&from.get().to_le_bytes());
                output[16..18].copy_from_slice(&to.get().to_le_bytes());
            }
        }
    }

    Ok(required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CommandId, NodeId, StationId, TrainId};
    use crate::message::{CommandKind, EmergencyReason, RejectionReason, StopReason};

    fn node(value: u16) -> NodeId {
        NodeId::new(value).unwrap()
    }

    fn train(value: u16) -> TrainId {
        TrainId::new(value).unwrap()
    }

    fn station(value: u16) -> StationId {
        StationId::new(value).unwrap()
    }

    fn command_id(value: u32) -> Option<CommandId> {
        Some(CommandId::new(value).unwrap())
    }

    const DEPART_ID: u32 = 0x01020304;
    const EMERGENCY_ID: u32 = 0x01020305;

    fn frame(message: Message, id: Option<CommandId>, reply: bool) -> Frame {
        Frame {
            source: node(if reply { 2 } else { 1 }),
            destination: node(if reply { 1 } else { 2 }),
            command_id: id,
            message,
        }
    }

    fn depart() -> Frame {
        frame(
            Message::Command(Command::Depart {
                train_id: train(3),
                from: station(16),
                to: station(32),
            }),
            command_id(DEPART_ID),
            false,
        )
    }

    fn emergency_stop() -> Frame {
        frame(
            Message::Command(Command::EmergencyStop {
                train_id: train(3),
                reason: EmergencyReason::OperatorRequest,
            }),
            command_id(EMERGENCY_ID),
            false,
        )
    }

    fn accepted(kind: CommandKind, id: u32) -> Frame {
        frame(
            Message::CommandResult(CommandResult::Accepted {
                train_id: train(3),
                command_kind: kind,
            }),
            command_id(id),
            true,
        )
    }

    fn rejected(kind: CommandKind, reason: RejectionReason, id: u32) -> Frame {
        frame(
            Message::CommandResult(CommandResult::Rejected {
                train_id: train(3),
                command_kind: kind,
                reason,
            }),
            command_id(id),
            true,
        )
    }

    fn departed() -> Frame {
        frame(
            Message::Event(Event::Departed {
                train_id: train(3),
                from: station(16),
                to: station(32),
            }),
            command_id(DEPART_ID),
            true,
        )
    }

    fn stopped(reason: StopReason, location: Location, id: Option<CommandId>) -> Frame {
        frame(
            Message::Event(Event::Stopped {
                train_id: train(3),
                reason,
                location,
            }),
            id,
            true,
        )
    }

    fn segment() -> Location {
        Location::OnSegment {
            from: station(16),
            to: station(32),
        }
    }

    // Verify exact wire bytes, reported length and untouched tail.
    fn assert_wire(frame: &Frame, expected: &[u8]) {
        let mut output = [0xAAu8; 32];

        let written = encode(frame, &mut output).unwrap();

        assert_eq!(written, expected.len());
        assert_eq!(&output[..written], expected);
        assert!(
            output[written..].iter().all(|&b| b == 0xAA),
            "Encoder modified unused buffer bytes"
        );
    }

    // An error must never modify the caller's buffer.
    fn assert_invalid(frame: &Frame) {
        let mut output = [0xAAu8; 32];

        assert_eq!(encode(frame, &mut output), Err(EncodeError::InvalidValue));
        assert!(output.iter().all(|&b| b == 0xAA));
    }

    // ---------------------------------------------------------
    // 1. Fixed binary vectors from SPEC-001, section 10
    // ---------------------------------------------------------

    #[test]
    fn encode_depart_matches_spec() {
        assert_wire(
            &depart(),
            &[
                0x01, 0x01, 0x01, 0x00, 0x02, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x10, 0x00,
                0x20, 0x00,
            ],
        );
    }

    #[test]
    fn encode_emergency_stop_matches_spec() {
        assert_wire(
            &emergency_stop(),
            &[
                0x01, 0x02, 0x01, 0x00, 0x02, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01,
            ],
        );
    }

    #[test]
    fn encode_accepted_depart_matches_spec() {
        assert_wire(
            &accepted(CommandKind::Depart, DEPART_ID),
            &[
                0x01, 0x10, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01,
            ],
        );
    }

    #[test]
    fn encode_accepted_emergency_matches_spec() {
        assert_wire(
            &accepted(CommandKind::EmergencyStop, EMERGENCY_ID),
            &[
                0x01, 0x10, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x02,
            ],
        );
    }

    #[test]
    fn encode_rejected_depart_matches_spec() {
        assert_wire(
            &rejected(
                CommandKind::Depart,
                RejectionReason::LocationMismatch,
                DEPART_ID,
            ),
            &[
                0x01, 0x11, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01, 0x02,
            ],
        );
    }

    #[test]
    fn encode_rejected_emergency_matches_spec() {
        assert_wire(
            &rejected(
                CommandKind::EmergencyStop,
                RejectionReason::UnableToExecute,
                EMERGENCY_ID,
            ),
            &[
                0x01, 0x11, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x02, 0x06,
            ],
        );
    }

    #[test]
    fn encode_departed_matches_spec() {
        assert_wire(
            &departed(),
            &[
                0x01, 0x20, 0x02, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x10, 0x00,
                0x20, 0x00,
            ],
        );
    }

    #[test]
    fn encode_stopped_emergency_matches_spec() {
        assert_wire(
            &stopped(
                StopReason::EmergencyCommand,
                segment(),
                command_id(EMERGENCY_ID),
            ),
            &[
                0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x05, 0x03, 0x02, 0x01, 0x03, 0x00, 0x01, 0x02,
                0x10, 0x00, 0x20, 0x00,
            ],
        );
    }

    #[test]
    fn encode_stopped_obstacle_matches_spec() {
        assert_wire(
            &stopped(StopReason::Obstacle, segment(), None),
            &[
                0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x02, 0x02,
                0x10, 0x00, 0x20, 0x00,
            ],
        );
    }

    #[test]
    fn encode_stopped_at_station_matches_spec() {
        assert_wire(
            &stopped(
                StopReason::LocalStop,
                Location::AtStation(station(16)),
                None,
            ),
            &[
                0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x03, 0x01,
                0x10, 0x00, 0x00, 0x00,
            ],
        );
    }

    #[test]
    fn encode_stopped_unknown_location_matches_spec() {
        assert_wire(
            &stopped(StopReason::Unspecified, Location::Unknown, None),
            &[
                0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00,
                0x00, 0x00, 0x00, 0x00,
            ],
        );
    }

    // ---------------------------------------------------------
    // 2. Buffer boundaries and error atomicity
    // ---------------------------------------------------------

    #[test]
    fn exact_sized_buffers_succeed() {
        let frames = [
            depart(),
            emergency_stop(),
            accepted(CommandKind::Depart, DEPART_ID),
            rejected(
                CommandKind::Depart,
                RejectionReason::InvalidState,
                DEPART_ID,
            ),
            departed(),
            stopped(StopReason::Obstacle, segment(), None),
        ];

        for frame in frames {
            let required = frame.message.kind().encoded_len();
            let mut buffer = [0xAAu8; 32];

            let result = encode(&frame, &mut buffer[..required]);
            assert_eq!(result, Ok(required));
        }
    }

    #[test]
    fn all_smaller_buffers_fail_without_mutation() {
        let frames = [
            depart(),
            emergency_stop(),
            accepted(CommandKind::Depart, DEPART_ID),
            rejected(
                CommandKind::Depart,
                RejectionReason::InvalidState,
                DEPART_ID,
            ),
            departed(),
            stopped(StopReason::Obstacle, segment(), None),
        ];

        for frame in frames {
            let required = frame.message.kind().encoded_len();

            for available in 0..required {
                let mut buffer = [0xAAu8; 32];

                let result = encode(&frame, &mut buffer[..available]);

                assert_eq!(
                    result,
                    Err(EncodeError::BufferTooSmall {
                        required,
                        available,
                    }),
                );

                assert!(buffer.iter().all(|&b| b == 0xAA));
            }
        }
    }

    // ---------------------------------------------------------
    // 3. Invalid message invariants
    // ---------------------------------------------------------

    #[test]
    fn rejects_same_source_and_destination() {
        let mut frames = [
            depart(),
            emergency_stop(),
            accepted(CommandKind::Depart, DEPART_ID),
            rejected(
                CommandKind::Depart,
                RejectionReason::InvalidState,
                DEPART_ID,
            ),
            departed(),
            stopped(StopReason::Obstacle, segment(), None),
        ];

        for frame in &mut frames {
            frame.destination = frame.source;
            assert_invalid(frame);
        }
    }

    #[test]
    fn rejects_missing_command_id_for_correlated_messages() {
        let mut frames = [
            depart(),
            emergency_stop(),
            accepted(CommandKind::Depart, DEPART_ID),
            rejected(
                CommandKind::Depart,
                RejectionReason::InvalidState,
                DEPART_ID,
            ),
            departed(),
        ];

        for frame in &mut frames {
            frame.command_id = None;
            assert_invalid(frame);
        }
    }

    #[test]
    fn rejects_depart_with_same_stations() {
        let mut frame = depart();

        if let Message::Command(Command::Depart { from, to, .. }) = &mut frame.message {
            *to = *from;
        }

        assert_invalid(&frame);
    }

    #[test]
    fn rejects_departed_with_same_stations() {
        let mut frame = departed();

        if let Message::Event(Event::Departed { from, to, .. }) = &mut frame.message {
            *to = *from;
        }

        assert_invalid(&frame);
    }

    #[test]
    fn rejects_invalid_emergency_rejection_reason() {
        let frame = rejected(
            CommandKind::EmergencyStop,
            RejectionReason::LocationMismatch,
            EMERGENCY_ID,
        );

        assert_invalid(&frame);
    }

    #[test]
    fn rejects_emergency_stop_event_without_command_id() {
        let frame = stopped(StopReason::EmergencyCommand, segment(), None);

        assert_invalid(&frame);
    }

    #[test]
    fn rejects_unsolicited_stop_with_command_id() {
        for reason in [
            StopReason::Obstacle,
            StopReason::LocalStop,
            StopReason::Unspecified,
        ] {
            let frame = stopped(reason, Location::Unknown, command_id(DEPART_ID));

            assert_invalid(&frame);
        }
    }

    #[test]
    fn rejects_same_segment_endpoints() {
        let frame = stopped(
            StopReason::Obstacle,
            Location::OnSegment {
                from: station(16),
                to: station(16),
            },
            None,
        );

        assert_invalid(&frame);
    }

    // ---------------------------------------------------------
    // 4. Validation order, boundaries and repeatability
    // ---------------------------------------------------------

    #[test]
    fn invalid_values_take_priority_over_small_buffers() {
        let mut frame = depart();
        frame.command_id = None;

        let mut buffer = [0xAAu8; 1];

        assert_eq!(encode(&frame, &mut buffer), Err(EncodeError::InvalidValue),);
        assert_eq!(buffer, [0xAA]);
    }

    #[test]
    fn invalid_stopped_location_takes_priority_over_small_buffer() {
        let frame = stopped(
            StopReason::Obstacle,
            Location::OnSegment {
                from: station(16),
                to: station(16),
            },
            None,
        );

        let mut buffer = [0xAAu8; 1];

        assert_eq!(encode(&frame, &mut buffer), Err(EncodeError::InvalidValue),);
        assert_eq!(buffer, [0xAA]);
    }

    #[test]
    fn maximum_valid_command_id_is_encoded_little_endian() {
        let mut frame = depart();
        frame.command_id = command_id(u32::MAX);

        let mut buffer = [0u8; 32];
        assert_eq!(encode(&frame, &mut buffer), Ok(16));

        assert_eq!(&buffer[6..10], &[0xFF; 4]);
    }

    #[test]
    fn maximum_valid_train_id_is_encoded_little_endian() {
        let mut frame = depart();

        if let Message::Command(Command::Depart { train_id, .. }) = &mut frame.message {
            *train_id = train(u16::MAX);
        }

        let mut buffer = [0u8; 32];
        assert_eq!(encode(&frame, &mut buffer), Ok(16));

        assert_eq!(&buffer[10..12], &[0xFF, 0xFF]);
    }

    #[test]
    fn node_and_train_ids_are_independent() {
        let mut frame = depart();
        frame.source = node(3);
        frame.destination = node(4);

        let mut buffer = [0u8; 32];
        assert_eq!(encode(&frame, &mut buffer), Ok(16));

        assert_eq!(&buffer[2..4], &[3, 0]);
        assert_eq!(&buffer[4..6], &[4, 0]);
        assert_eq!(&buffer[10..12], &[3, 0]);
    }

    #[test]
    fn encoding_is_deterministic() {
        let frame = depart();

        let mut first = [0xAAu8; 32];
        let mut second = [0xBBu8; 32];

        let n1 = encode(&frame, &mut first).unwrap();
        let n2 = encode(&frame, &mut second).unwrap();

        assert_eq!(n1, n2);
        assert_eq!(&first[..n1], &second[..n2]);
    }
}
