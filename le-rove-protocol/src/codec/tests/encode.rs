use super::*;

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
    assert_wire(&depart(), &DEPART);
}

#[test]
fn encode_emergency_stop_matches_spec() {
    assert_wire(&emergency_stop(), &EMERGENCY);
}

#[test]
fn encode_accepted_depart_matches_spec() {
    assert_wire(&accepted(CommandKind::Depart, DEPART_ID), &ACCEPTED_DEPART);
}

#[test]
fn encode_accepted_emergency_matches_spec() {
    assert_wire(
        &accepted(CommandKind::EmergencyStop, EMERGENCY_ID),
        &ACCEPTED_EMERGENCY,
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
        &REJECTED_DEPART,
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
        &REJECTED_EMERGENCY,
    );
}

#[test]
fn encode_departed_matches_spec() {
    assert_wire(&departed(), &DEPARTED);
}

#[test]
fn encode_stopped_emergency_matches_spec() {
    assert_wire(
        &stopped(
            StopReason::EmergencyCommand,
            segment(),
            command_id(EMERGENCY_ID),
        ),
        &STOPPED_EMERGENCY,
    );
}

#[test]
fn encode_stopped_obstacle_matches_spec() {
    assert_wire(
        &stopped(StopReason::Obstacle, segment(), None),
        &STOPPED_OBSTACLE,
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
        &STOPPED_STATION,
    );
}

#[test]
fn encode_stopped_unknown_location_matches_spec() {
    assert_wire(
        &stopped(StopReason::Unspecified, Location::Unknown, None),
        &STOPPED_UNKNOWN,
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
