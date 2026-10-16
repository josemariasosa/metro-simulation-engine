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
