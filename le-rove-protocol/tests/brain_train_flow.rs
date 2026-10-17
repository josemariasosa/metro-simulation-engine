//! In-memory protocol exchanges between independent consumer roles.
//!
//! Codec validation is structural and locally semantic: decoding does not authorize
//! a command or establish that it is safe. Endpoint authorization and command-history
//! correlation belong to consumers. Accepted does not prove physical movement;
//! Departed and Stopped represent reported observations, supplied here as test data.
//! The codec provides no retransmission, session recovery, or delivery guarantees.
//! Host tests emulate the Train API usage; the separate embedded library build
//! verifies no_std compatibility. No motor, sensor, or transport behavior is modeled.
use le_rove_protocol::{
    codec::{EncodeError, decode, encode},
    frame::Frame,
    identity::{CommandId, NodeId, StationId, TrainId},
    message::{
        Command, CommandKind, CommandResult, EmergencyReason, Event, Location, Message,
        MessageKind, RejectionReason, StopReason,
    },
};

fn brain_command(message: Message, id: u32) -> Frame {
    Frame {
        source: NodeId::new(1).unwrap(),
        destination: NodeId::new(2).unwrap(),
        command_id: Some(CommandId::new(id).unwrap()),
        message,
    }
}

fn departure() -> Frame {
    brain_command(
        Message::Command(Command::Depart {
            train_id: TrainId::new(3).unwrap(),
            from: StationId::new(16).unwrap(),
            to: StationId::new(32).unwrap(),
        }),
        42,
    )
}

// Only encoded bytes cross the endpoint boundary; no Frame is handed to a receiver.
fn exchange(sent: &Frame) -> Frame {
    let mut wire = [0u8; 32];
    let len = encode(sent, &mut wire).unwrap();
    decode(&wire[..len]).unwrap()
}

fn train_reply(received: &Frame, message: Message) -> Frame {
    Frame {
        source: received.destination,
        destination: received.source,
        command_id: received.command_id,
        message,
    }
}

// These are consumer checks against retained command history, not codec guarantees.
fn assert_correlated_reply(reply: &Frame, sent: &Frame) {
    assert_eq!(reply.source, sent.destination);
    assert_eq!(reply.destination, sent.source);
    assert_eq!(reply.command_id, sent.command_id);
    assert_ne!(reply.command_id.unwrap().get(), 0);
}

#[test]
fn departure_is_accepted_then_reported() {
    let sent = departure();
    let received = exchange(&sent);
    assert_eq!(received, sent);
    let Message::Command(Command::Depart { train_id, from, to }) = received.message else {
        panic!("Train expected Depart");
    };
    let accepted = exchange(&train_reply(
        &received,
        Message::CommandResult(CommandResult::Accepted {
            train_id,
            command_kind: CommandKind::Depart,
        }),
    ));
    assert_correlated_reply(&accepted, &sent);
    assert_eq!(
        accepted.message,
        Message::CommandResult(CommandResult::Accepted {
            train_id: TrainId::new(3).unwrap(),
            command_kind: CommandKind::Depart,
        })
    );
    // A separate reported observation, not an inference from acceptance.
    let departed = exchange(&train_reply(
        &received,
        Message::Event(Event::Departed { train_id, from, to }),
    ));
    assert_correlated_reply(&departed, &sent);
    assert_eq!(
        departed.message,
        Message::Event(Event::Departed {
            train_id: TrainId::new(3).unwrap(),
            from: StationId::new(16).unwrap(),
            to: StationId::new(32).unwrap(),
        })
    );
}

#[test]
fn departure_rejected_by_local_safety_inhibit() {
    let sent = departure();
    let received = exchange(&sent);
    assert_eq!(received, sent);
    let Message::Command(Command::Depart { train_id, .. }) = received.message else {
        panic!("Train expected Depart");
    };
    let rejected = exchange(&train_reply(
        &received,
        Message::CommandResult(CommandResult::Rejected {
            train_id,
            command_kind: CommandKind::Depart,
            reason: RejectionReason::SafetyInhibit,
        }),
    ));
    assert_correlated_reply(&rejected, &sent);
    assert_eq!(
        rejected.message,
        Message::CommandResult(CommandResult::Rejected {
            train_id: TrainId::new(3).unwrap(),
            command_kind: CommandKind::Depart,
            reason: RejectionReason::SafetyInhibit,
        })
    );
    // This consumer scenario ends at rejection; it generates no Departed event.
}

#[test]
fn emergency_acceptance_and_reported_stop_are_distinct() {
    let sent = brain_command(
        Message::Command(Command::EmergencyStop {
            train_id: TrainId::new(3).unwrap(),
            reason: EmergencyReason::OperatorRequest,
        }),
        43,
    );
    let received = exchange(&sent);
    assert_eq!(received, sent);
    let Message::Command(Command::EmergencyStop { train_id, reason }) = received.message else {
        panic!("Train expected EmergencyStop");
    };
    assert_eq!(reason, EmergencyReason::OperatorRequest);
    let accepted = exchange(&train_reply(
        &received,
        Message::CommandResult(CommandResult::Accepted {
            train_id,
            command_kind: CommandKind::EmergencyStop,
        }),
    ));
    assert_correlated_reply(&accepted, &sent);
    assert_eq!(
        accepted.message,
        Message::CommandResult(CommandResult::Accepted {
            train_id: TrainId::new(3).unwrap(),
            command_kind: CommandKind::EmergencyStop,
        })
    );
    let location = Location::OnSegment {
        from: StationId::new(16).unwrap(),
        to: StationId::new(32).unwrap(),
    };
    let stopped = exchange(&train_reply(
        &received,
        Message::Event(Event::Stopped {
            train_id,
            reason: StopReason::EmergencyCommand,
            location,
        }),
    ));
    assert_correlated_reply(&stopped, &sent);
    assert_eq!(
        stopped.message,
        Message::Event(Event::Stopped {
            train_id: TrainId::new(3).unwrap(),
            reason: StopReason::EmergencyCommand,
            location,
        })
    );
    assert_eq!(accepted.message.kind(), MessageKind::Accepted);
    assert_eq!(stopped.message.kind(), MessageKind::Stopped);
}

#[test]
fn spontaneous_obstacle_stop_has_no_command_correlation() {
    let event = Frame {
        source: NodeId::new(2).unwrap(),
        destination: NodeId::new(1).unwrap(),
        command_id: None,
        message: Message::Event(Event::Stopped {
            train_id: TrainId::new(3).unwrap(),
            reason: StopReason::Obstacle,
            location: Location::OnSegment {
                from: StationId::new(16).unwrap(),
                to: StationId::new(32).unwrap(),
            },
        }),
    };
    let mut wire = [0u8; 32];
    let len = encode(&event, &mut wire).unwrap();
    // SPEC-001 common header: command_id is the four bytes at offset 6.
    assert_eq!(&wire[6..10], &[0, 0, 0, 0]);
    let received = decode(&wire[..len]).unwrap();
    assert_eq!(received.command_id, None);
    assert_eq!(received, event);
    let incorrectly_correlated = Frame {
        command_id: CommandId::new(42),
        ..event
    };
    assert_eq!(
        encode(&incorrectly_correlated, &mut wire),
        Err(EncodeError::InvalidValue)
    );
}
