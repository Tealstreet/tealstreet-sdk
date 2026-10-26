//! Paradex schema 1:2. Layout: paradex-py/paradex_py/api/sbe/paradex_1_0.xml.
//! Fixed fields use the advertised block length; variable fields remain borrowed.
#[derive(Clone, Copy)]
pub(crate) struct Span<'a>(pub(crate) &'a [u8]);
impl Span<'_> {
    pub(crate) fn len(self) -> usize {
        self.0.len()
    }
    pub(crate) fn u8(self, at: usize) -> Result<u8, &'static str> {
        self.0.get(at).copied().ok_or("Truncated SBE field")
    }
    pub(crate) fn i64(self, at: usize) -> Result<i64, &'static str> {
        Ok(i64::from_le_bytes(
            self.0
                .get(at..at.checked_add(8).ok_or("Invalid SBE offset")?)
                .ok_or("Truncated SBE field")?
                .try_into()
                .map_err(|_| "Invalid SBE field")?,
        ))
    }
    pub(crate) fn f8(self, at: usize) -> Result<Option<f64>, &'static str> {
        Ok(scale(self.i64(at)?, 1e8))
    }
    pub(crate) fn f12(self, at: usize) -> Result<Option<f64>, &'static str> {
        Ok(scale(self.i64(at)?, 1e12))
    }
}
fn scale(n: i64, factor: f64) -> Option<f64> {
    (n != i64::MIN).then_some(n as f64 / factor)
}

pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
}
impl<'a> Cursor<'a> {
    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len()
    }
    pub(crate) fn string(&mut self) -> Result<&'a str, &'static str> {
        let len = usize::from(*self.bytes.first().ok_or("Truncated SBE string")?);
        let value = std::str::from_utf8(self.bytes.get(1..1 + len).ok_or("Truncated SBE string")?)
            .map_err(|_| "Invalid SBE UTF-8")?;
        self.bytes = &self.bytes[1 + len..];
        Ok(value)
    }
    pub(crate) fn group(&mut self, min_block: usize) -> Result<Group<'a>, &'static str> {
        let header = self.bytes.get(..4).ok_or("Truncated SBE group")?;
        let block = usize::from(u16::from_le_bytes([header[0], header[1]]));
        let count = usize::from(u16::from_le_bytes([header[2], header[3]]));
        if block < min_block {
            return Err("Invalid SBE group block");
        }
        let end = 4 + block.checked_mul(count).ok_or("Invalid SBE group length")?;
        let bytes = self.bytes.get(4..end).ok_or("Truncated SBE group")?;
        self.bytes = &self.bytes[end..];
        Ok(Group { bytes, block })
    }
}
pub(crate) struct Group<'a> {
    bytes: &'a [u8],
    block: usize,
}
impl<'a> Group<'a> {
    pub(crate) fn rows(&self) -> impl Iterator<Item = Span<'a>> {
        self.bytes.chunks(self.block).map(Span)
    }
}
pub(crate) struct Frame<'a> {
    pub(crate) template: u16,
    pub(crate) version: u16,
    pub(crate) root: Span<'a>,
    pub(crate) tail: Cursor<'a>,
}
impl<'a> Frame<'a> {
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Self, &'static str> {
        let h = bytes.get(..8).ok_or("Truncated Paradex SBE header")?;
        let block = usize::from(u16::from_le_bytes([h[0], h[1]]));
        let template = u16::from_le_bytes([h[2], h[3]]);
        let schema = u16::from_le_bytes([h[4], h[5]]);
        let version = u16::from_le_bytes([h[6], h[7]]);
        if schema != 1 || !(1..=2).contains(&version) {
            return Err("Unsupported Paradex SBE schema");
        }
        let root = Span(
            bytes
                .get(8..8 + block)
                .ok_or("Truncated Paradex SBE block")?,
        );
        Ok(Self {
            template,
            version,
            root,
            tail: Cursor {
                bytes: &bytes[8 + block..],
            },
        })
    }
}

pub(crate) enum Public<'a> {
    Trade(super::wire::Trade<'a>),
    Bbo(super::wire::Bbo<'a>),
    Summary(Box<super::wire::Tick<'a>>),
    Book(super::wire::Depth<'a>),
    Heartbeat,
    Other,
}
fn required(value: Option<f64>) -> Result<f64, &'static str> {
    value.ok_or("Null mandatory SBE field")
}

pub(crate) fn public(bytes: &[u8]) -> Result<Public<'_>, &'static str> {
    use super::wire::{Bbo, Depth, Level, Number, StrictNumber, Tick, Trade};
    let mut f = Frame::parse(bytes)?;
    let decoded = match f.template {
        1 => {
            if f.root.len() < 50 {
                return Err("Truncated SBE trade block");
            }
            let price = required(f.root.f8(25)?)?;
            let size = required(f.root.f8(33)?)?;
            let created_at = f.root.i64(41)?;
            if created_at < 0 {
                return Err("Invalid SBE trade time");
            }
            let market = f.tail.string()?;
            if f.version >= 2 && f.tail.remaining() > 0 {
                let _ = f.tail.string()?;
            }
            Public::Trade(Trade {
                market: market.into(),
                price: Number(price),
                size: Number(size),
                created_at: Number((created_at / 1000) as f64),
            })
        }
        2 => {
            if f.root.len() < 48 {
                return Err("Truncated SBE BBO block");
            }
            Public::Bbo(Bbo {
                market: f.tail.string()?.into(),
                bid_price: Number(f.root.f8(16)?.unwrap_or(0.)),
                ask_price: Number(f.root.f8(32)?.unwrap_or(0.)),
                index_price: Number(0.),
            })
        }
        3 => {
            if f.root.len() < 89 {
                return Err("Truncated SBE book block");
            }
            let sequence = f.root.i64(8)?;
            if sequence < 0 {
                return Err("Invalid SBE book sequence");
            }
            let kind = match f.root.u8(16)? {
                0 => "s",
                1 => "d",
                _ => return Err("Invalid SBE book package"),
            };
            let bids = f.tail.group(16)?;
            let asks = f.tail.group(16)?;
            let market = f.tail.string()?;
            let mut levels = Vec::new();
            for (side, group) in [("BUY", bids), ("SELL", asks)] {
                for row in group.rows() {
                    let price = required(row.f8(0)?)?;
                    let size = required(row.f8(8)?)?;
                    if price <= 0. || size < 0. {
                        return Err("Invalid SBE book level");
                    }
                    levels.push(Level {
                        side: side.into(),
                        price: StrictNumber(price),
                        size: StrictNumber(size),
                    });
                }
            }
            Public::Book(Depth {
                market: market.into(),
                seq_no: sequence as u64,
                update_type: kind.into(),
                inserts: levels,
                updates: vec![],
                deletes: vec![],
            })
        }
        4 => {
            let minimum = if f.version >= 1 { 240 } else { 216 };
            if f.root.len() < minimum {
                return Err("Truncated SBE summary block");
            }
            let funding = if f.root.len() >= 240 {
                f.root.f12(232)?.or(f.root.f8(56)?)
            } else {
                f.root.f8(56)?
            };
            Public::Summary(Box::new(Tick {
                symbol: f.tail.string()?.into(),
                mark_price: Number(required(f.root.f8(16)?)?),
                underlying_price: Number(required(f.root.f8(24)?)?),
                last_traded_price: Number(required(f.root.f8(32)?)?),
                volume_24h: Number(required(f.root.f8(40)?)?),
                open_interest: Number(required(f.root.f8(48)?)?),
                funding_rate: Number(required(funding)?),
                bid: Number(f.root.f8(64)?.unwrap_or(0.)),
                ask: Number(f.root.f8(72)?.unwrap_or(0.)),
                price_change_rate_24h: Number(required(f.root.f8(88)?)?),
                ..Tick::default()
            }))
        }
        40 => {
            if f.root.len() < 16 {
                return Err("Truncated SBE heartbeat");
            }
            Public::Heartbeat
        }
        _ => return Ok(Public::Other),
    };
    if f.tail.remaining() != 0 {
        return Err("Unexpected SBE trailing data");
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(template: u16, block: &[u8], tail: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        for n in [block.len() as u16, template, 1, 2] {
            b.extend(n.to_le_bytes());
        }
        b.extend(block);
        b.extend(tail);
        b
    }
    #[test]
    fn rejects_truncated_headers_blocks_and_unknown_versions() {
        for len in 0..8 {
            assert!(Frame::parse(&vec![0; len]).is_err());
        }
        let mut b = frame(40, &[0; 16], &[]);
        b[6] = 3;
        assert!(Frame::parse(&b).is_err());
        b[6] = 2;
        b[0] = 17;
        assert!(Frame::parse(&b).is_err());
    }
    #[test]
    fn rejects_short_or_overflowing_groups_and_invalid_utf8() {
        assert!(Frame::parse(&frame(3, &[], &[])).is_ok());
        let b = frame(3, &[], &[15, 0, 1, 0]);
        let mut f = Frame::parse(&b).unwrap();
        assert!(f.tail.group(16).is_err());
        let b = frame(3, &[], &[255, 255, 255, 255]);
        let mut f = Frame::parse(&b).unwrap();
        assert!(f.tail.group(16).is_err());
        let b = frame(2, &[0; 48], &[1, 255]);
        assert!(public(&b).is_err());
    }
}
