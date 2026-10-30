//! VENUE_STATE for frame-fed Paradex: the data source's native order rows.
//!
//! Amend re-signs an order from its exact venue row (`Account::native_order`),
//! which the store's mapped `Order` does not carry. v1 is `[1, count u32]`
//! then per row: 16 length-prefixed UTF-8 fields, flags (0 absent, else
//! 1 + count + strings), and reduceOnly/postOnly (0 absent, 1 false, 2 true).
use crate::account::NativeOrder;

const VERSION: u8 = 1;
/// The source exports at most this many rows; more refuses on decode.
pub(crate) const MAX_ROWS: usize = 4096;

fn text(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn flag(out: &mut Vec<u8>, value: Option<bool>) {
    out.push(match value {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    });
}

fn fields(row: &NativeOrder) -> [&str; 16] {
    [
        &row.id,
        &row.market,
        &row.side,
        &row.kind,
        &row.status,
        &row.size,
        &row.remaining_size,
        &row.price,
        &row.trigger_price,
        &row.instruction,
        &row.time_in_force,
        &row.on_behalf_of_account,
        &row.client_id,
        &row.cancel_reason,
        &row.created_at,
        &row.last_updated_at,
    ]
}

pub(crate) fn encode<'a>(rows: impl ExactSizeIterator<Item = &'a NativeOrder>) -> Vec<u8> {
    let mut out = vec![VERSION];
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for row in rows {
        for field in fields(row) {
            text(&mut out, field);
        }
        match &row.flags {
            None => out.push(0),
            Some(flags) => {
                out.push(1);
                out.extend_from_slice(&(flags.len() as u32).to_le_bytes());
                for flag in flags {
                    text(&mut out, flag);
                }
            }
        }
        flag(&mut out, row.reduce_only);
        flag(&mut out, row.post_only);
    }
    out
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        if self.0.len() < n {
            return Err("truncated Paradex venue state");
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, &'static str> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<usize, &'static str> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }
    fn text(&mut self) -> Result<String, &'static str> {
        let len = self.u32()?;
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(|_| "invalid Paradex venue state text")
    }
    fn flag(&mut self) -> Result<Option<bool>, &'static str> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(false)),
            2 => Ok(Some(true)),
            _ => Err("invalid Paradex venue state flag"),
        }
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<NativeOrder>, &'static str> {
    let mut reader = Reader(bytes);
    if reader.u8()? != VERSION {
        return Err("unsupported Paradex venue state version");
    }
    let count = reader.u32()?;
    if count > MAX_ROWS {
        return Err("too many Paradex venue state rows");
    }
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let mut text = [(); 16].map(|_| String::new());
        for field in &mut text {
            *field = reader.text()?;
        }
        let [
            id,
            market,
            side,
            kind,
            status,
            size,
            remaining_size,
            price,
            trigger_price,
            instruction,
            time_in_force,
            on_behalf_of_account,
            client_id,
            cancel_reason,
            created_at,
            last_updated_at,
        ] = text;
        let flags = match reader.u8()? {
            0 => None,
            1 => {
                let n = reader.u32()?;
                if n > 64 {
                    return Err("too many Paradex order flags");
                }
                Some(
                    (0..n)
                        .map(|_| reader.text())
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }
            _ => return Err("invalid Paradex venue state flags"),
        };
        rows.push(NativeOrder {
            id,
            market,
            side,
            kind,
            status,
            size,
            remaining_size,
            price,
            trigger_price,
            flags,
            reduce_only: reader.flag()?,
            instruction,
            post_only: reader.flag()?,
            time_in_force,
            on_behalf_of_account,
            client_id,
            cancel_reason,
            created_at,
            last_updated_at,
        });
    }
    if !reader.0.is_empty() {
        return Err("trailing bytes after Paradex venue state");
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, flags: Option<Vec<String>>, reduce_only: Option<bool>) -> NativeOrder {
        NativeOrder {
            id: id.into(),
            market: "BTC-USD-PERP".into(),
            side: "BUY".into(),
            kind: "LIMIT".into(),
            status: "OPEN".into(),
            size: "0.5".into(),
            remaining_size: "0.25".into(),
            price: "60000".into(),
            instruction: "GTC".into(),
            client_id: "c-é".into(),
            created_at: "1717171717171".into(),
            last_updated_at: "1717171717999".into(),
            flags,
            reduce_only,
            post_only: Some(false),
            ..NativeOrder::default()
        }
    }

    #[test]
    fn rows_round_trip_exactly() {
        let rows = [
            row("1", Some(vec!["REDUCE_ONLY".into()]), Some(true)),
            row("2", None, None),
            row("3", Some(Vec::new()), Some(false)),
        ];
        let decoded = decode(&encode(rows.iter())).unwrap();
        assert_eq!(format!("{decoded:?}"), format!("{:?}", rows.to_vec()));
        assert!(decode(&encode(std::iter::empty())).unwrap().is_empty());
    }

    #[test]
    fn another_version_truncation_and_trailing_bytes_refuse() {
        let mut bytes = encode([row("1", None, None)].iter());
        let mut other = bytes.clone();
        other[0] = 2;
        assert_eq!(
            decode(&other).unwrap_err(),
            "unsupported Paradex venue state version"
        );
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
        bytes.push(0);
        assert_eq!(
            decode(&bytes).unwrap_err(),
            "trailing bytes after Paradex venue state"
        );
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn an_oversized_count_refuses_before_allocating() {
        let mut bytes = vec![VERSION];
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            decode(&bytes).unwrap_err(),
            "too many Paradex venue state rows"
        );
    }
}
