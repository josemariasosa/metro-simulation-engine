use crate::frame::Frame;
use crate::message::{Command, CommandResult, Event, Message};

pub enum EncodeError {
    InvalidValue,
    BufferTooSmall { required: usize, available: usize },
}

/// Mensaje	        Bytes
/// Depart	        16
/// EmergencyStop	13
/// Accepted	    13
/// Rejected	    14
/// Departed	    16
/// Stopped	        18
pub fn encode(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    match &frame.message {
        Message::Command(Command::Depart { train_id, from, to }) => {
            if frame.command_id.is_none() || frame.source == frame.destination || from == to {
                return Err(EncodeError::InvalidValue);
            }
            if output.len() < 16 {
                return Err(EncodeError::BufferTooSmall {
                    required: 16,
                    available: output.len(),
                });
            }
        }
        _ => todo!("Other message encoders"),
    }

    output[0] = crate::constants::PROTOCOL_VERSION;
    output[1] = frame.message.kind() as u8;
    output[2..4].copy_from_slice(&frame.source.get().to_le_bytes());
    output[4..6].copy_from_slice(&frame.destination.get().to_le_bytes());

    Ok(1)
    // Further encoding logic goes here
}
