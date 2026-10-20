use safe_cex_engine::SocketId;

pub(crate) struct MessageBatch<'a> {
    remaining: &'a [u8],
}

impl<'a> MessageBatch<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }
}

impl<'a> Iterator for MessageBatch<'a> {
    type Item = Result<(SocketId, f64, &'a str), &'static str>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let data = std::mem::take(&mut self.remaining);
        let Some(header) = data.get(..16) else {
            return Some(Err("truncated websocket batch header"));
        };
        let socket = u32::from_le_bytes(header[..4].try_into().expect("four bytes"));
        let length = u32::from_le_bytes(header[4..8].try_into().expect("four bytes")) as usize;
        let now = f64::from_le_bytes(header[8..16].try_into().expect("eight bytes"));
        if !now.is_finite() {
            return Some(Err("invalid websocket arrival time"));
        }
        let Some((text, rest)) = data[16..].split_at_checked(length) else {
            return Some(Err("truncated websocket batch text"));
        };
        match std::str::from_utf8(text) {
            Ok(text) => {
                self.remaining = rest;
                Some(Ok((SocketId(socket), now, text)))
            }
            Err(_) => Some(Err("invalid websocket batch UTF-8")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Payload<'a> {
    Text(&'a str),
    Binary(&'a [u8]),
}

pub(crate) struct EventBatch<'a> {
    remaining: &'a [u8],
}

impl<'a> EventBatch<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }
}

impl<'a> Iterator for EventBatch<'a> {
    type Item = Result<(SocketId, f64, Payload<'a>), &'static str>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let data = std::mem::take(&mut self.remaining);
        let Some(header) = data.get(..20) else {
            return Some(Err("truncated websocket event header"));
        };
        let socket = SocketId(u32::from_le_bytes(
            header[..4].try_into().expect("four bytes"),
        ));
        let length = u32::from_le_bytes(header[4..8].try_into().expect("four bytes")) as usize;
        let now = f64::from_le_bytes(header[8..16].try_into().expect("eight bytes"));
        let kind = u32::from_le_bytes(header[16..20].try_into().expect("four bytes"));
        if !now.is_finite() {
            return Some(Err("invalid websocket arrival time"));
        }
        let Some((payload, rest)) = data[20..].split_at_checked(length) else {
            return Some(Err("truncated websocket event payload"));
        };
        let payload = match kind {
            0 => match std::str::from_utf8(payload) {
                Ok(text) => Payload::Text(text),
                Err(_) => return Some(Err("invalid websocket event UTF-8")),
            },
            1 => Payload::Binary(payload),
            _ => return Some(Err("invalid websocket event kind")),
        };
        self.remaining = rest;
        Some(Ok((socket, now, payload)))
    }
}

/// Validation is atomic: no callback can observe a valid prefix of a bad batch.
pub(crate) fn dispatch_events<'a>(
    data: &'a [u8],
    mut handle: impl FnMut(SocketId, f64, Payload<'a>),
) -> Result<(), &'static str> {
    for record in EventBatch::new(data) {
        record?;
    }
    for record in EventBatch::new(data) {
        let (socket, now, payload) = record?;
        handle(socket, now, payload);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrows_text_and_retains_order_and_time() {
        let mut data = Vec::new();
        for (socket, now, text) in [(7_u32, 12.5_f64, "\n雪😀"), (3, 11.0, "")] {
            data.extend(socket.to_le_bytes());
            data.extend((text.len() as u32).to_le_bytes());
            data.extend(now.to_le_bytes());
            data.extend(text.as_bytes());
        }
        let records = MessageBatch::new(&data)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            records,
            [(SocketId(7), 12.5, "\n雪😀"), (SocketId(3), 11.0, "")]
        );
        assert_eq!(records[0].2.as_ptr(), data[16..].as_ptr());
    }

    #[test]
    fn rejects_truncation_invalid_text_and_time() {
        assert_eq!(MessageBatch::new(&[]).count(), 0);
        for length in 1..16 {
            assert!(
                MessageBatch::new(&[0; 16][..length])
                    .next()
                    .unwrap()
                    .is_err()
            );
        }
        let mut data = vec![0; 16];
        data[4] = 1;
        assert!(MessageBatch::new(&data).next().unwrap().is_err());
        data.push(255);
        assert!(MessageBatch::new(&data).next().unwrap().is_err());
        data[16] = b'a';
        data[8..16].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(MessageBatch::new(&data).next().unwrap().is_err());
    }

    fn event(socket: u32, now: f64, kind: u32, payload: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(socket.to_le_bytes());
        data.extend((payload.len() as u32).to_le_bytes());
        data.extend(now.to_le_bytes());
        data.extend(kind.to_le_bytes());
        data.extend(payload);
        data
    }

    #[test]
    fn mixed_events_borrow_payloads_and_preserve_kind_socket_order_and_time() {
        let first = event(7, 12.5, 0, "雪😀".as_bytes());
        let binary_offset = first.len() + 20;
        let mut data = first;
        data.extend(event(u32::MAX, 11.0, 1, &[0, 255, 192, 175]));
        data.extend(event(0, -1.0, 0, b""));
        data.extend(event(3, 0.0, 1, b""));
        let events = EventBatch::new(&data)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            events,
            [
                (SocketId(7), 12.5, Payload::Text("雪😀")),
                (
                    SocketId(u32::MAX),
                    11.0,
                    Payload::Binary(&[0, 255, 192, 175])
                ),
                (SocketId(0), -1.0, Payload::Text("")),
                (SocketId(3), 0.0, Payload::Binary(&[])),
            ]
        );
        let Payload::Text(text) = events[0].2 else {
            panic!("text kind changed")
        };
        let Payload::Binary(bytes) = events[1].2 else {
            panic!("binary kind changed")
        };
        assert_eq!(text.as_ptr(), data[20..].as_ptr());
        assert_eq!(bytes.as_ptr(), data[binary_offset..].as_ptr());
        let mut dispatched = Vec::new();
        dispatch_events(&data, |socket, now, payload| {
            dispatched.push((socket, now, payload))
        })
        .unwrap();
        assert_eq!(dispatched, events);
        assert_eq!(EventBatch::new(&[]).count(), 0);
        dispatch_events(&[], |_, _, _| panic!("empty batch dispatched an event")).unwrap();
    }

    #[test]
    fn mixed_batch_errors_reject_the_entire_prefix_before_dispatch() {
        let mut prefix = event(1, 1.0, 0, b"valid");
        prefix.extend(event(2, 2.0, 1, &[255]));
        let mut malformed = (1..20).map(|n| vec![0; n]).collect::<Vec<_>>();
        malformed.push(event(3, 3.0, 2, b""));
        malformed.push(event(3, 3.0, u32::MAX, b""));
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            malformed.push(event(3, now, 0, b""));
        }
        for invalid in [&[255][..], &[192, 175], &[237, 160, 128], &[240, 159]] {
            malformed.push(event(3, 3.0, 0, invalid));
        }
        let mut truncated = event(3, 3.0, 1, b"tail");
        truncated.pop();
        malformed.push(truncated);
        let mut oversized = event(3, 3.0, 1, b"");
        oversized[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        malformed.push(oversized);
        for suffix in malformed {
            let mut data = prefix.clone();
            data.extend(suffix);
            let mut dispatched = Vec::new();
            assert!(
                dispatch_events(&data, |socket, now, payload| dispatched
                    .push((socket, now, payload)))
                .is_err()
            );
            assert!(
                dispatched.is_empty(),
                "invalid final event committed a prefix"
            );
        }
    }

    #[test]
    fn malformed_mixed_records_are_terminal_for_the_iterator() {
        let data = event(1, 1.0, 9, b"");
        let mut events = EventBatch::new(&data);
        assert!(events.next().unwrap().is_err());
        assert!(events.next().is_none());
    }
}
