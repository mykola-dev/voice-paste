//! Bounded response decoding, independent of process/OS ownership.
use std::{io::BufRead, sync::mpsc::SyncSender};
use voicepaste_protocol::Response;

pub(super) const RESPONSE_LIMIT: usize = 16;

pub(super) fn read_responses(
    mut reader: impl BufRead,
    sender: SyncSender<Result<Response, String>>,
) {
    loop {
        match voicepaste_protocol::read::<Response>(&mut reader) {
            Ok(Some(response)) => {
                // Do not block the pipe reader indefinitely on a stalled consumer.
                // Overflow closes the producer; the owner drains then sees disconnect.
                if sender.try_send(Ok(response)).is_err() {
                    break;
                }
            }
            result => {
                let error = match result {
                    Err(error) => error,
                    _ => "Native worker output closed".into(),
                };
                let _ = sender.try_send(Err(error));
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, TryRecvError};
    use voicepaste_protocol::Generation;

    #[test]
    fn overflow_is_bounded_ordered_and_eventually_disconnects() {
        let mut bytes = Vec::new();
        for index in 0..RESPONSE_LIMIT + 2 {
            voicepaste_protocol::write(
                &mut bytes,
                &Response::Error {
                    generation: Generation(1),
                    id: None,
                    message: index.to_string(),
                },
            )
            .unwrap();
        }
        let (sender, receiver) = mpsc::sync_channel(RESPONSE_LIMIT);
        read_responses(bytes.as_slice(), sender);
        for index in 0..RESPONSE_LIMIT {
            assert!(
                matches!(receiver.try_recv(), Ok(Ok(Response::Error {message, ..})) if message == index.to_string())
            );
        }
        assert!(matches!(
            receiver.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn eof_and_malformed_frames_report_an_error_then_disconnect() {
        for bytes in [b"".as_slice(), b"{bad}\n".as_slice()] {
            let (sender, receiver) = mpsc::sync_channel(RESPONSE_LIMIT);
            read_responses(bytes, sender);
            assert!(matches!(receiver.try_recv(), Ok(Err(_))));
            assert!(matches!(
                receiver.try_recv(),
                Err(TryRecvError::Disconnected)
            ));
        }
    }
}
