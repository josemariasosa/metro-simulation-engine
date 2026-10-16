//! External-consumer API smoke tests; exhaustive wire validation lives in unit tests.
use le_rove_protocol::{
    Command, CommandId, CommandKind, CommandResult, DecodeError, EmergencyReason, EncodeError,
    Event, Field, Frame, Location, Message, MessageKind, NodeId, RejectionReason, StationId,
    StopReason, TrainId, decode, encode,
};

fn departure() -> Frame {
    Frame {
        source: NodeId::new(1).unwrap(),
        destination: NodeId::new(2).unwrap(),
        command_id: CommandId::new(42),
        message: Message::Command(Command::Depart {
            train_id: TrainId::new(3).unwrap(),
            from: StationId::new(16).unwrap(),
            to: StationId::new(32).unwrap(),
        }),
    }
}

#[test]
fn all_six_message_kinds_are_usable_externally() {
    let train_id = TrainId::new(3).unwrap();
    let from = StationId::new(16).unwrap();
    let to = StationId::new(32).unwrap();
    let messages = [
        departure().message,
        Message::Command(Command::EmergencyStop {
            train_id,
            reason: EmergencyReason::OperatorRequest,
        }),
        Message::CommandResult(CommandResult::Accepted {
            train_id,
            command_kind: CommandKind::Depart,
        }),
        Message::CommandResult(CommandResult::Rejected {
            train_id,
            command_kind: CommandKind::Depart,
            reason: RejectionReason::SafetyInhibit,
        }),
        Message::Event(Event::Departed { train_id, from, to }),
        Message::Event(Event::Stopped {
            train_id,
            reason: StopReason::EmergencyCommand,
            location: Location::AtStation(from),
        }),
    ];
    let kinds = [
        MessageKind::Depart,
        MessageKind::EmergencyStop,
        MessageKind::Accepted,
        MessageKind::Rejected,
        MessageKind::Departed,
        MessageKind::Stopped,
    ];
    for (message, kind) in messages.into_iter().zip(kinds) {
        let command = matches!(message, Message::Command(_));
        let frame = Frame {
            source: NodeId::new(if command { 1 } else { 2 }).unwrap(),
            destination: NodeId::new(if command { 2 } else { 1 }).unwrap(),
            command_id: CommandId::new(42),
            message,
        };
        let mut output = [0u8; 32];
        let len = encode(&frame, &mut output).unwrap();
        let decoded = decode(&output[..len]).unwrap();
        assert_eq!(decoded, frame);
        assert_eq!(decoded.message.kind(), kind);
        let subject = match decoded.message {
            Message::Command(Command::Depart { train_id, .. })
            | Message::Command(Command::EmergencyStop { train_id, .. })
            | Message::CommandResult(CommandResult::Accepted { train_id, .. })
            | Message::CommandResult(CommandResult::Rejected { train_id, .. })
            | Message::Event(Event::Departed { train_id, .. })
            | Message::Event(Event::Stopped { train_id, .. }) => train_id,
        };
        assert_eq!(subject, train_id);
    }
}

#[test]
fn external_consumer_can_handle_decode_errors() {
    let mut bytes = [0u8; 32];
    let len = encode(&departure(), &mut bytes).unwrap();
    // Offsets are the public SPEC-001 v1 wire contract, not codec internals.
    let mut invalid = bytes;
    invalid[0] = 2;
    assert!(matches!(
        decode(&invalid[..len]),
        Err(DecodeError::UnsupportedVersion { actual: 2 })
    ));
    invalid = bytes;
    invalid[1] = 0xff;
    assert!(matches!(
        decode(&invalid[..len]),
        Err(DecodeError::UnknownMessageKind { actual: 0xff })
    ));
    assert_eq!(
        decode(&bytes[..len - 1]),
        Err(DecodeError::InvalidLength {
            expected: len,
            actual: len - 1
        })
    );
    assert!(matches!(
        decode(&bytes[..9]),
        Err(DecodeError::HeaderTooShort { actual: 9 })
    ));
    invalid = bytes;
    invalid[6..10].fill(0);
    assert!(matches!(
        decode(&invalid[..len]),
        Err(DecodeError::InvalidValue {
            field: Field::CommandId
        })
    ));
    assert!(matches!(
        CommandKind::try_from(0xff),
        Err(DecodeError::InvalidDiscriminant {
            field: Field::CommandKind,
            actual: 0xff
        })
    ));
}

#[test]
fn external_consumer_can_handle_encode_errors() {
    let frame = departure();
    let mut short = [0u8; 1];
    assert_eq!(
        encode(&frame, &mut short),
        Err(EncodeError::BufferTooSmall {
            required: MessageKind::Depart.encoded_len(),
            available: 1,
        })
    );
    let mut output = [0u8; 32];
    assert!(matches!(
        encode(
            &Frame {
                command_id: None,
                ..frame
            },
            &mut output
        ),
        Err(EncodeError::InvalidValue)
    ));
}
