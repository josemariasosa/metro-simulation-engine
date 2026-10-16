use super::*;

#[test]
fn all_message_kinds_roundtrip() {
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
        let mut buffer = [0u8; 32];
        let written = encode(&frame, &mut buffer).unwrap();
        let decoded = decode(&buffer[..written]).unwrap();

        assert_eq!(decoded, frame);
    }
}

#[test]
fn brain_safety_decision_roundtrip() {
    let frame = frame(
        Message::Command(Command::EmergencyStop {
            train_id: train(3),
            reason: EmergencyReason::BrainSafetyDecision,
        }),
        command_id(EMERGENCY_ID),
        false,
    );
    let mut expected = EMERGENCY;
    expected[12] = 0x02;

    let mut buffer = [0u8; 32];
    let written = encode(&frame, &mut buffer).unwrap();
    assert_eq!(&buffer[..written], &expected);
    assert_eq!(decode(&buffer[..written]).unwrap(), frame);
}

#[test]
fn remaining_depart_rejection_reasons_roundtrip() {
    // InvalidState and LocationMismatch are covered by the existing round trip
    // and fixed vectors; the emergency rejection vector covers UnableToExecute.
    for (reason, discriminant) in [
        (RejectionReason::UnknownStation, 0x03),
        (RejectionReason::UnsupportedMovement, 0x04),
        (RejectionReason::SafetyInhibit, 0x05),
        (RejectionReason::UnableToExecute, 0x06),
    ] {
        let frame = rejected(CommandKind::Depart, reason, DEPART_ID);
        let mut expected = REJECTED_DEPART;
        expected[13] = discriminant;

        let mut buffer = [0u8; 32];
        let written = encode(&frame, &mut buffer).unwrap();
        assert_eq!(&buffer[..written], &expected);
        assert_eq!(decode(&buffer[..written]).unwrap(), frame);
    }
}

#[test]
fn identity_boundaries_roundtrip_with_exact_wire_bytes() {
    // Swap endpoints so both node and station fields exercise both boundaries
    // without violating the distinct-endpoint invariants.
    for (value, other, id, bytes, other_bytes, id_bytes) in [
        (
            1,
            u16::MAX,
            1,
            [0x01, 0x00],
            [0xFF, 0xFF],
            [0x01, 0x00, 0x00, 0x00],
        ),
        (u16::MAX, 1, u32::MAX, [0xFF, 0xFF], [0x01, 0x00], [0xFF; 4]),
    ] {
        let mut frame = frame(
            Message::Command(Command::Depart {
                train_id: train(value),
                from: station(value),
                to: station(other),
            }),
            command_id(id),
            false,
        );
        frame.source = node(value);
        frame.destination = node(other);

        let mut buffer = [0u8; 32];
        let written = encode(&frame, &mut buffer).unwrap();
        assert_eq!(written, DEPART.len());
        assert_eq!(&buffer[2..4], &bytes);
        assert_eq!(&buffer[4..6], &other_bytes);
        assert_eq!(&buffer[6..10], &id_bytes);
        assert_eq!(&buffer[10..12], &bytes);
        assert_eq!(&buffer[12..14], &bytes);
        assert_eq!(&buffer[14..16], &other_bytes);
        assert_eq!(decode(&buffer[..written]).unwrap(), frame);
    }
}
