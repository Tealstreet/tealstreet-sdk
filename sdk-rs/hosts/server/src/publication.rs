//! Per-registration public-data publication. Private sections never wait here.
use safe_cex_engine::{
    Store,
    book::{Book, BookSide},
    book_output,
    ingest::{self, Cursor, Frame, IngestError, IngestState},
    wire::{self, FrameWriter, section},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

pub const MIN_INTERVAL_MS: u16 = 50;
pub const MAX_INTERVAL_MS: u16 = 5000;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RequestedInterval {
    #[default]
    Absent,
    Milliseconds(u16),
    Invalid,
}
impl RequestedInterval {
    pub fn parse(value: &Value) -> Self {
        let millis = value
            .as_i64()
            .map(|n| n.clamp(MIN_INTERVAL_MS as i64, MAX_INTERVAL_MS as i64) as u16)
            .or_else(|| {
                value
                    .as_u64()
                    .map(|n| n.clamp(MIN_INTERVAL_MS as u64, MAX_INTERVAL_MS as u64) as u16)
            });
        millis.map_or(Self::Invalid, Self::Milliseconds)
    }
    pub fn millis(&self) -> Option<u16> {
        match self {
            Self::Absent => Some(0),
            Self::Milliseconds(value) => Some((*value).clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS)),
            Self::Invalid => None,
        }
    }
}
impl<'de> Deserialize<'de> for RequestedInterval {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&Value::deserialize(deserializer)?))
    }
}

fn public(kind: u16) -> bool {
    matches!(
        kind,
        section::TICKERS
            | section::TICKER_DELTAS
            | section::TICKER_OBSERVATIONS
            | section::BOOK
            | section::BOOK_DELTA
    )
}

/// Keep each source frame's own string references and all unknown/private sections.
fn filter(bytes: &[u8], keep_public: bool) -> Result<Option<Vec<u8>>, IngestError> {
    select(bytes, |kind| public(kind) == keep_public)
}

fn select(bytes: &[u8], keep: impl Fn(u16) -> bool) -> Result<Option<Vec<u8>>, IngestError> {
    let parts = parts(bytes)?;
    let kept: Vec<_> = parts
        .into_iter()
        .filter(|(kind, _)| *kind == section::STRINGS || keep(*kind))
        .collect();
    if !kept.iter().any(|(kind, _)| *kind != section::STRINGS) {
        return Ok(None);
    }
    let mut result = bytes[..16].to_vec();
    result[6..8].copy_from_slice(&(kept.len() as u16).to_le_bytes());
    for (_, part) in kept {
        result.extend_from_slice(part);
    }
    Ok(Some(result))
}

fn parts(bytes: &[u8]) -> Result<Vec<(u16, &[u8])>, IngestError> {
    ingest::parse(bytes)?;
    let count = u16::from_le_bytes([bytes[6], bytes[7]]);
    let mut at = 16usize;
    let mut parts = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let header = bytes
            .get(
                at..at
                    .checked_add(8)
                    .ok_or(IngestError("invalid publication section"))?,
            )
            .ok_or(IngestError("invalid publication section"))?;
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let end = at
            .checked_add(8)
            .and_then(|n| n.checked_add(length))
            .ok_or(IngestError("invalid publication section"))?;
        let part = bytes
            .get(at..end)
            .ok_or(IngestError("invalid publication section"))?;
        parts.push((kind, part));
        at = end;
    }
    if at != bytes.len() {
        return Err(IngestError("invalid publication trailing bytes"));
    }
    Ok(parts)
}

fn split(bytes: Vec<u8>, limit: usize) -> Result<Vec<Arc<[u8]>>, IngestError> {
    if bytes.len() <= limit {
        return Ok(vec![bytes.into()]);
    }
    let parts = parts(&bytes)?;
    let strings = parts
        .iter()
        .find(|(kind, _)| *kind == section::STRINGS)
        .map_or(&[][..], |(_, part)| *part);
    let mut result = Vec::new();
    let mut current = bytes[..16].to_vec();
    let mut count = 0u16;
    let finish = |mut current: Vec<u8>, count: u16| {
        current.extend_from_slice(strings);
        let count = count + u16::from(!strings.is_empty());
        current[6..8].copy_from_slice(&count.to_le_bytes());
        Arc::from(current)
    };
    for (kind, part) in parts {
        if kind == section::STRINGS {
            continue;
        }
        if 16 + strings.len() + part.len() > limit {
            return Err(IngestError("publication section exceeds connection limit"));
        }
        if current.len() + part.len() + strings.len() > limit {
            result.push(finish(current, count));
            current = bytes[..16].to_vec();
            count = 0;
        }
        current.extend_from_slice(part);
        count += 1;
    }
    if count > 0 {
        result.push(finish(current, count));
    }
    Ok(result)
}

#[derive(Default)]
struct PublicState {
    store: Store,
    source: IngestState,
    books: HashMap<String, Book>,
    sent_books: HashMap<String, Book>,
    dirty_books: HashSet<String>,
    reset_books: HashSet<String>,
    observations: BTreeMap<u32, [f64; 4]>,
    loaded: Option<u32>,
    stamp: f64,
    pending: bool,
}
impl PublicState {
    fn observe(&mut self, bytes: &[u8]) -> Result<bool, IngestError> {
        let frame = ingest::parse(bytes)?;
        self.stamp = f64::from_le_bytes(bytes[8..16].try_into().unwrap());
        if frame.sections.iter().any(|s| s.kind == section::TICKERS) {
            self.observations.clear();
        }
        let selected = Frame {
            sections: frame
                .sections
                .iter()
                .filter(|s| {
                    matches!(
                        s.kind,
                        section::MARKETS
                            | section::TICKERS
                            | section::TICKER_DELTAS
                            | section::TICKER_OBSERVATIONS
                    )
                })
                .copied()
                .collect(),
            strings: frame.strings,
        };
        ingest::apply(&mut self.store, &mut self.source, &selected, false)?;
        if frame.sections.iter().any(|s| s.kind == section::MARKETS) {
            let symbols: HashSet<_> = self
                .source
                .source_markets
                .iter()
                .map(String::as_str)
                .collect();
            self.books.retain(|name, _| symbols.contains(name.as_str()));
            self.sent_books
                .retain(|name, _| symbols.contains(name.as_str()));
            self.dirty_books
                .retain(|name| symbols.contains(name.as_str()));
            self.reset_books
                .retain(|name| symbols.contains(name.as_str()));
        }
        let mut barrier = false;
        for part in &frame.sections {
            match part.kind {
                section::BOOK | section::BOOK_DELTA => {
                    if part.revision != 0 {
                        return Err(IngestError("unknown publication book revision"));
                    }
                    let mut cursor = Cursor::new(part.payload);
                    let market = cursor.u32()?;
                    let symbol = self.source.symbol(market)?.to_owned();
                    let bids = cursor.u32()? as usize;
                    let asks = cursor.u32()? as usize;
                    cursor.u32()?;
                    let full = part.kind == section::BOOK;
                    let stride = if full { 24 } else { 16 };
                    if bids
                        .checked_add(asks)
                        .and_then(|n| n.checked_mul(stride))
                        .and_then(|n| n.checked_add(16))
                        != Some(part.payload.len())
                    {
                        return Err(IngestError("invalid publication book section"));
                    }
                    if full {
                        self.books.entry(symbol.clone()).or_default().clear();
                        self.reset_books.insert(symbol.clone());
                    }
                    let book = self
                        .books
                        .get_mut(&symbol)
                        .ok_or(IngestError("publication book delta has no baseline"))?;
                    for (side, count) in [(BookSide::Bids, bids), (BookSide::Asks, asks)] {
                        for _ in 0..count {
                            let price = cursor.f64()?;
                            let amount = cursor.f64()?;
                            if full {
                                cursor.f64()?;
                            }
                            book.apply(side, price, amount);
                        }
                    }
                    self.dirty_books.insert(symbol);
                }
                section::TICKER_OBSERVATIONS => {
                    for row in part.payload.as_chunks::<32>().0 {
                        let mut cursor = Cursor::new(row);
                        let values = [cursor.f64()?, cursor.f64()?, cursor.f64()?, cursor.f64()?];
                        self.observations.insert(values[0] as u32, values);
                    }
                }
                section::STATUS => {
                    let loaded = Cursor::new(part.payload).u32()?;
                    barrier |= self.loaded.is_some_and(|old| old != loaded);
                    self.loaded = Some(loaded);
                }
                _ => {}
            }
            self.pending |= public(part.kind);
        }
        Ok(barrier)
    }
    fn baseline(&mut self) {
        self.sent_books = self
            .books
            .iter()
            .map(|(symbol, book)| (symbol.clone(), book.clone()))
            .collect();
    }
    fn acknowledge(&mut self, paced: bool) {
        self.store.take_pending();
        for symbol in &self.dirty_books {
            if let Some(book) = self.books.get_mut(symbol).filter(|book| book.is_empty()) {
                *book = Book::default();
            }
        }
        if paced {
            for symbol in &self.dirty_books {
                if let Some(book) = self.books.get(symbol) {
                    self.sent_books.insert(symbol.clone(), book.clone());
                }
            }
        } else {
            self.sent_books.clear();
        }
        self.dirty_books.clear();
        self.reset_books.clear();
        self.observations.clear();
        self.pending = false;
    }
    fn flush(&mut self, limit: usize) -> Result<Vec<Arc<[u8]>>, IngestError> {
        if !self.pending {
            return Ok(Vec::new());
        }
        let pending = self.store.take_pending();
        let mut writer = FrameWriter::new(Vec::new(), self.stamp);
        self.store.write(&pending, &mut writer);
        if !self.observations.is_empty() {
            writer.begin(section::TICKER_OBSERVATIONS);
            for row in self.observations.values() {
                writer.f64s(*row);
            }
            writer.end();
        }
        let mut symbols: Vec<_> = self.dirty_books.iter().collect();
        symbols.sort_unstable();
        for symbol in symbols {
            let Some(market) = self.source.source_markets.iter().position(|s| s == symbol) else {
                continue;
            };
            let book = &self.books[symbol];
            if let Some(previous) = self
                .sent_books
                .get(symbol)
                .filter(|_| !self.reset_books.contains(symbol))
            {
                book_output::write_delta(book, previous, market as u32, &mut writer);
            } else {
                book_output::write(book, market as u32, &mut writer);
            }
        }
        let encoded = writer.finish();
        let frames = match filter(&encoded, true)? {
            Some(encoded) => split(encoded, limit)?,
            None => Vec::new(),
        };
        self.acknowledge(true);
        Ok(frames)
    }
}

/// This state belongs to one socket/account pump, never to the shared account actor.
pub struct Publisher {
    interval: u16,
    last_publication: Instant,
    state: Option<PublicState>,
    limit: usize,
}
impl Publisher {
    pub fn new(interval: u16, limit: usize) -> Self {
        Self {
            interval,
            last_publication: Instant::now(),
            state: Some(PublicState::default()),
            limit,
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        (self.interval > 0 && self.state.as_ref().is_some_and(|state| state.pending))
            .then(|| self.last_publication + Duration::from_millis(self.interval as u64))
    }
    pub fn flush(&mut self) -> Result<Vec<Arc<[u8]>>, IngestError> {
        let frames = self
            .state
            .as_mut()
            .ok_or(IngestError("publication baseline unavailable"))?
            .flush(self.limit)?;
        if !frames.is_empty() {
            self.last_publication = Instant::now();
        }
        Ok(frames)
    }
    pub fn set_interval(&mut self, interval: u16) -> Result<Vec<Arc<[u8]>>, IngestError> {
        if self.interval == interval {
            return Ok(Vec::new());
        }
        if interval == 0 {
            let frames = self.flush()?;
            self.interval = 0;
            return Ok(frames);
        }
        let state = self
            .state
            .as_mut()
            .ok_or(IngestError("publication baseline unavailable"))?;
        if self.interval == 0 {
            state.baseline();
        }
        self.interval = interval;
        if self.deadline().is_some_and(|at| at <= Instant::now()) {
            self.flush()
        } else {
            Ok(Vec::new())
        }
    }
    pub fn receive(
        &mut self,
        bytes: Arc<[u8]>,
        initial: bool,
    ) -> Result<Vec<Arc<[u8]>>, IngestError> {
        let observed = self
            .state
            .as_mut()
            .ok_or(IngestError("publication baseline unavailable"))
            .and_then(|state| state.observe(&bytes));
        let barrier = match observed {
            Ok(barrier) => barrier,
            Err(error) => {
                self.state = None;
                // Old clients retain their exact bytes and cadence even on an unknown format.
                if self.interval == 0 {
                    return Ok(vec![bytes]);
                }
                return Err(error);
            }
        };
        if initial || self.interval == 0 {
            self.state.as_mut().unwrap().acknowledge(self.interval > 0);
            self.last_publication = Instant::now();
            return Ok(vec![bytes]);
        }
        // Catalogue/private state precedes dependent books; readiness follows its public data.
        // Ordinary deadlines run separately, after immediate output has been sent.
        let mut result = Vec::new();
        if barrier && self.state.as_ref().is_some_and(|state| state.pending) {
            if let Some(immediate) =
                select(&bytes, |kind| !public(kind) && kind != section::STATUS)?
            {
                result.push(immediate.into());
            }
            result.extend(self.flush()?);
            if let Some(status) = select(&bytes, |kind| kind == section::STATUS)? {
                result.push(status.into());
            }
        } else if wire::sections(&bytes).any(|(kind, _)| public(kind)) {
            if let Some(immediate) = filter(&bytes, false)? {
                result.push(immediate.into());
            }
        } else {
            result.push(bytes);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
