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
}
