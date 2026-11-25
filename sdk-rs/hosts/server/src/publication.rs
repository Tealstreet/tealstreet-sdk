//! Per-registration public-data publication. Private sections never wait here.
use safe_cex_engine::{
    Store,
    book::{Book, BookSide},
    book_output,
    ingest::{self, Cursor, Frame, IngestError, IngestState},
    model::Ticker,
    store::Pending,
    wire::{self, FrameWriter, section},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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

pub const MAX_TICKER_SYMBOLS: usize = 1024;
pub const MAX_TICKER_SYMBOL_BYTES: usize = 128;

/// The ticker rows one connection receives: `None` is the venue's full table.
pub type TickerSelection = Option<Arc<BTreeSet<String>>>;

/// `activeTickerSymbols`: absent or null is the full table, a list selects rows by symbol.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum RequestedTickers {
    #[default]
    Absent,
    Symbols(Arc<BTreeSet<String>>),
    Invalid,
}
impl RequestedTickers {
    pub fn parse(value: &Value) -> Self {
        let Some(items) = value.as_array() else {
            return if value.is_null() {
                Self::Absent
            } else {
                Self::Invalid
            };
        };
        if items.len() > MAX_TICKER_SYMBOLS {
            return Self::Invalid;
        }
        let symbols: Option<BTreeSet<String>> = items
            .iter()
            .map(|item| {
                item.as_str()
                    .filter(|symbol| (1..=MAX_TICKER_SYMBOL_BYTES).contains(&symbol.len()))
                    .map(str::to_owned)
            })
            .collect();
        symbols.map_or(Self::Invalid, |symbols| Self::Symbols(Arc::new(symbols)))
    }
    pub fn selection(&self) -> Option<TickerSelection> {
        match self {
            Self::Absent => Some(None),
            Self::Symbols(symbols) => Some(Some(symbols.clone())),
            Self::Invalid => None,
        }
    }
}
impl<'de> Deserialize<'de> for RequestedTickers {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&Value::deserialize(deserializer)?))
    }
}

/// One connection's publication choices; never engine settings or account identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preferences {
    pub interval: u16,
    pub tickers: TickerSelection,
}

fn ticker_section(kind: u16) -> bool {
    matches!(
        kind,
        section::TICKERS | section::TICKER_DELTAS | section::TICKER_OBSERVATIONS
    )
}

/// The selected rows of the source table, in source order, as their own ticker table.
/// Its `TICKERS` replaces the client's table; its deltas use output rows.
struct TickerFilter {
    symbols: Arc<BTreeSet<String>>,
    out: Store,
    rows: Vec<u32>,
    by_source: HashMap<u32, u32>,
}
impl TickerFilter {
    fn new(symbols: Arc<BTreeSet<String>>, source: &[Ticker]) -> Self {
        let mut filter = Self {
            symbols,
            out: Store::default(),
            rows: Vec::new(),
            by_source: HashMap::new(),
        };
        filter.rebuild(source);
        filter
    }
    fn rebuild(&mut self, source: &[Ticker]) {
        self.rows = (0..source.len() as u32)
            .filter(|&row| self.symbols.contains(&source[row as usize].symbol))
            .collect();
        self.by_source = self
            .rows
            .iter()
            .enumerate()
            .map(|(out, &row)| (row, out as u32))
            .collect();
        self.out.set_tickers(
            self.rows
                .iter()
                .map(|&row| source[row as usize].clone())
                .collect(),
        );
    }
    fn refresh(&mut self, source: &[Ticker]) {
        for (out, &row) in self.rows.iter().enumerate() {
            let Some(next) = source.get(row as usize) else {
                continue;
            };
            self.out.update_ticker_at(out as u32, |ticker| {
                ticker.bid = next.bid;
                ticker.ask = next.ask;
                ticker.last = next.last;
                ticker.mark = next.mark;
                ticker.index = next.index;
                ticker.mid_price = next.mid_price;
                ticker.percentage = next.percentage;
                ticker.open_interest = next.open_interest;
                ticker.funding_rate = next.funding_rate;
                ticker.funding_time = next.funding_time;
                ticker.volume = next.volume;
                ticker.quote_volume = next.quote_volume;
            });
        }
    }
}

fn write_observations<'a>(
    writer: &mut FrameWriter<'a>,
    rows: &BTreeMap<u32, [f64; 4]>,
    remap: Option<&HashMap<u32, u32>>,
) {
    let mut selected: Vec<[f64; 4]> = rows
        .iter()
        .filter_map(|(row, values)| match remap {
            None => Some(*values),
            Some(remap) => remap.get(row).map(|out| {
                let mut values = *values;
                values[0] = *out as f64;
                values
            }),
        })
        .collect();
    if selected.is_empty() {
        return;
    }
    selected.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]));
    writer.begin(section::TICKER_OBSERVATIONS);
    for row in selected {
        writer.f64s(row);
    }
    writer.end();
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
    /// Latest observation per source row, for rows a new selection starts sending.
    observed: BTreeMap<u32, [f64; 4]>,
    filter: Option<TickerFilter>,
    /// A cleared selection owes the client the full table once.
    replace_tickers: bool,
    loaded: Option<u32>,
    stamp: f64,
    pending: bool,
}
impl PublicState {
    fn observe(&mut self, bytes: &[u8]) -> Result<bool, IngestError> {
        let frame = ingest::parse(bytes)?;
        self.stamp = f64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let replaced = frame.sections.iter().any(|s| s.kind == section::TICKERS);
        if replaced {
            self.observations.clear();
            self.observed.clear();
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
        if let Some(filter) = &mut self.filter {
            if replaced {
                filter.rebuild(self.store.tickers());
            } else if frame
                .sections
                .iter()
                .any(|s| s.kind == section::TICKER_DELTAS)
            {
                filter.refresh(self.store.tickers());
            }
        }
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
                        self.observed.insert(values[0] as u32, values);
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
    fn select_tickers(&mut self, selection: TickerSelection) {
        match selection {
            Some(symbols) => {
                self.filter = Some(TickerFilter::new(symbols, self.store.tickers()));
                self.replace_tickers = false;
            }
            None => self.replace_tickers |= self.filter.take().is_some(),
        }
        self.observations = self.observed.clone();
    }
    fn take_ticker_pending(&mut self) -> Pending {
        let source = self.store.take_pending();
        match &mut self.filter {
            Some(filter) => filter.out.take_pending(),
            None if self.replace_tickers => self.store.everything(),
            None => source,
        }
    }
    /// Ticker sections only matter here; callers keep just the public kinds.
    fn write_tickers<'a>(&'a self, pending: &Pending, writer: &mut FrameWriter<'a>) {
        match &self.filter {
            Some(filter) => {
                filter.out.write(pending, writer);
                write_observations(writer, &self.observations, Some(&filter.by_source));
            }
            None => {
                self.store.write(pending, writer);
                write_observations(writer, &self.observations, None);
            }
        }
    }
    /// An immediate frame with a selection: its other sections keep their bytes, and
    /// its ticker sections become the selected rows, ahead of any readiness change.
    fn split_tickers(&mut self, bytes: &[u8], limit: usize) -> Result<Vec<Arc<[u8]>>, IngestError> {
        let pending = self.take_ticker_pending();
        let mut writer = FrameWriter::new(Vec::new(), self.stamp);
        self.write_tickers(&pending, &mut writer);
        let tickers = match filter(&writer.finish(), true)? {
            Some(encoded) => split(encoded, limit)?,
            None => Vec::new(),
        };
        if tickers.is_empty() {
            return Ok(select(bytes, |kind| !ticker_section(kind))?
                .map(Arc::from)
                .into_iter()
                .collect());
        }
        let mut frames: Vec<Arc<[u8]>> = Vec::new();
        if let Some(head) = select(bytes, |kind| {
            !ticker_section(kind) && kind != section::STATUS
        })? {
            frames.push(head.into());
        }
        frames.extend(tickers);
        if let Some(status) = select(bytes, |kind| kind == section::STATUS)? {
            frames.push(status.into());
        }
        Ok(frames)
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
        if let Some(filter) = &mut self.filter {
            filter.out.take_pending();
        }
        self.replace_tickers = false;
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
        let pending = self.take_ticker_pending();
        let mut writer = FrameWriter::new(Vec::new(), self.stamp);
        self.write_tickers(&pending, &mut writer);
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
    tickers: TickerSelection,
    /// Set once the first (snapshot) frame has gone out; nothing publishes before it.
    started: bool,
    /// The client holds a selected, renumbered ticker table: raw source rows would misapply.
    selected_table: bool,
    last_publication: Instant,
    state: Option<PublicState>,
    limit: usize,
}
impl Publisher {
    pub fn new(interval: u16, limit: usize) -> Self {
        Self {
            interval,
            tickers: None,
            started: false,
            selected_table: false,
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
    /// A changed selection publishes at once rather than at the next pacing deadline.
    pub fn set_tickers(
        &mut self,
        selection: TickerSelection,
    ) -> Result<Vec<Arc<[u8]>>, IngestError> {
        if self.tickers == selection {
            return Ok(Vec::new());
        }
        let state = self
            .state
            .as_mut()
            .ok_or(IngestError("publication baseline unavailable"))?;
        self.tickers = selection.clone();
        state.select_tickers(selection);
        if !self.started {
            return Ok(Vec::new());
        }
        state.pending = true;
        let frames = self.flush()?;
        self.selected_table = self.tickers.is_some();
        Ok(frames)
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
                // A selected table cannot take source rows: it resnapshots, and the new pump's
                // snapshot then passes through whole, replacing the client's table.
                if self.interval == 0 && (initial || !self.selected_table) {
                    if initial {
                        self.started = true;
                        self.selected_table = false;
                    }
                    return Ok(vec![bytes]);
                }
                return Err(error);
            }
        };
        if initial || self.interval == 0 {
            let state = self.state.as_mut().unwrap();
            let frames = if state.filter.is_some()
                && wire::sections(&bytes).any(|(kind, _)| ticker_section(kind))
            {
                self.selected_table = true;
                state.split_tickers(&bytes, self.limit)?
            } else {
                vec![bytes]
            };
            state.acknowledge(self.interval > 0);
            self.last_publication = Instant::now();
            self.started = true;
            return Ok(frames);
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
