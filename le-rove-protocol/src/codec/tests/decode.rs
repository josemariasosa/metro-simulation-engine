use super::*;

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
    let expected = frame(
        Message::Command(Command::Depart {
            train_id: train(3),
            from: station(16),
            to: station(32),
        }),
        command_id(0x01020304),
        false,
    );

    assert_eq!(decode(&DEPART), Ok(expected));
}

#[test]
fn decode_emergency_stop_matches_spec() {
    let expected = frame(
        Message::Command(Command::EmergencyStop {
            train_id: train(3),
            reason: EmergencyReason::OperatorRequest,
        }),
        command_id(0x01020305),
        false,
    );

    assert_eq!(decode(&EMERGENCY), Ok(expected));
}

#[test]
fn decode_accepted_depart_matches_spec() {
    let expected = frame(
        Message::CommandResult(CommandResult::Accepted {
            train_id: train(3),
            command_kind: CommandKind::Depart,
        }),
        command_id(0x01020304),
        true,
    );

    assert_eq!(decode(&ACCEPTED_DEPART), Ok(expected));
}

#[test]
fn decode_accepted_emergency_matches_spec() {
    let expected = frame(
        Message::CommandResult(CommandResult::Accepted {
            train_id: train(3),
            command_kind: CommandKind::EmergencyStop,
        }),
        command_id(0x01020305),
        true,
    );

    assert_eq!(decode(&ACCEPTED_EMERGENCY), Ok(expected));
}

#[test]
fn decode_rejected_depart_matches_spec() {
    let expected = frame(
        Message::CommandResult(CommandResult::Rejected {
            train_id: train(3),
            command_kind: CommandKind::Depart,
            reason: RejectionReason::LocationMismatch,
        }),
        command_id(0x01020304),
        true,
    );

    assert_eq!(decode(&REJECTED_DEPART), Ok(expected));
}

#[test]
fn decode_rejected_emergency_matches_spec() {
    let expected = frame(
        Message::CommandResult(CommandResult::Rejected {
            train_id: train(3),
            command_kind: CommandKind::EmergencyStop,
            reason: RejectionReason::UnableToExecute,
        }),
        command_id(0x01020305),
        true,
    );

    assert_eq!(decode(&REJECTED_EMERGENCY), Ok(expected));
}

#[test]
fn decode_departed_matches_spec() {
    let expected = frame(
        Message::Event(Event::Departed {
            train_id: train(3),
            from: station(16),
            to: station(32),
        }),
        command_id(0x01020304),
        true,
    );

    assert_eq!(decode(&DEPARTED), Ok(expected));
}

#[test]
fn decode_stopped_emergency_matches_spec() {
    let expected = frame(
        Message::Event(Event::Stopped {
            train_id: train(3),
            reason: StopReason::EmergencyCommand,
            location: segment(),
        }),
        command_id(0x01020305),
        true,
    );

    assert_eq!(decode(&STOPPED_EMERGENCY), Ok(expected));
}

#[test]
fn decode_stopped_obstacle_matches_spec() {
    let expected = frame(
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
    let expected = frame(
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
    let expected = frame(
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
