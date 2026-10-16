use crate::frame::Frame;
use crate::message::{
    Command, CommandKind, CommandResult, Event, Location, Message, MessageKind, RejectionReason,
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

    if !frame.has_valid_command_id() {
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
        if from == to {
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
        if *command_kind == CommandKind::EmergencyStop
            && *reason != RejectionReason::UnableToExecute
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
        if from == to {
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
        if let Location::OnSegment { from, to } = location
            && from == to
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
