use super::*;
use safe_cex_engine::model::OrderStatus;

fn financial_section(kind: u16) -> bool {
    matches!(kind, section::ORDERS | section::POSITIONS)
}

impl PublicState {
    pub(super) fn observe_financial(&mut self, frame: &Frame<'_>) -> Result<bool, IngestError> {
        if !frame
            .sections
            .iter()
            .any(|part| financial_section(part.kind))
        {
            return Ok(false);
        }
        let selected = Frame {
            sections: frame
                .sections
                .iter()
                .filter(|part| financial_section(part.kind))
                .copied()
                .collect(),
            strings: frame.strings,
        };
        if ingest::apply(&mut self.store, &mut self.source, &selected, false).is_err() {
            self.financial_unreadable = true;
        }
        if self.financial_unreadable {
            return if self.selection.is_some() {
                Err(IngestError("financial ticker baseline unavailable"))
            } else {
                Ok(false)
            };
        }
        let mut required = BTreeSet::new();
        for position in self.store.positions() {
            if position.contracts != 0. || position.notional != 0. {
                required.insert(position.symbol.clone());
            }
        }
        for order in self.store.orders() {
            if !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                required.insert(order.symbol.clone());
            }
        }
        let changed = self.financial != required;
        self.financial = required;
        Ok(changed)
    }

    pub(super) fn sync_financial_filter(&mut self) -> bool {
        let effective = self.selection.as_ref().map(|symbols| {
            let mut required = (**symbols).clone();
            required.extend(self.financial.iter().cloned());
            required.extend(self.published_financial.iter().cloned());
            Arc::new(required)
        });
        if self.filter.as_ref().map(|filter| &filter.symbols) == effective.as_ref() {
            return false;
        }
        match effective {
            Some(symbols) => {
                self.filter = Some(TickerFilter::new(symbols, self.store.tickers()));
                self.replace_tickers = false;
            }
            None => self.replace_tickers |= self.filter.take().is_some(),
        }
        self.observations = self.observed.clone();
        true
    }

    fn financial_market_snapshot(&self, limit: usize) -> Result<Vec<Arc<[u8]>>, IngestError> {
        // A financial row may use a catalogue that public pacing has not sent yet.
        let mut catalogue = Store::default();
        catalogue.set_markets(self.store.markets().to_vec());
        let pending = catalogue.take_pending();
        let mut writer = FrameWriter::new(Vec::new(), self.stamp);
        catalogue.write(&pending, &mut writer);
        match select(&writer.finish(), |kind| kind == section::MARKETS)? {
            Some(bytes) => split(bytes, limit),
            None => Ok(Vec::new()),
        }
    }

    fn financial_ticker_snapshot(&self, limit: usize) -> Result<Vec<Arc<[u8]>>, IngestError> {
        let filter = self
            .filter
            .as_ref()
            .ok_or(IngestError("selected ticker baseline unavailable"))?;
        let pending = filter.out.everything();
        let mut writer = FrameWriter::new(Vec::new(), self.stamp);
        filter.out.write(&pending, &mut writer);
        write_observations(&mut writer, &self.observed, Some(&filter.by_source));
        match select(&writer.finish(), ticker_section)? {
            Some(bytes) => split(bytes, limit),
            None => Ok(Vec::new()),
        }
    }

    pub(super) fn project_financial(
        &mut self,
        frames: Vec<Arc<[u8]>>,
        limit: usize,
        initial: bool,
    ) -> Result<Vec<Arc<[u8]>>, IngestError> {
        if !frames
            .iter()
            .any(|bytes| wire::sections(bytes).any(|(kind, _)| financial_section(kind)))
        {
            return Ok(frames);
        }
        // Keep private data in the first snapshot chunk; its ticker union precedes readiness.
        if initial || self.filter.is_none() || self.financial == self.published_financial {
            self.published_financial = self.financial.clone();
            return Ok(frames);
        }
        let mut output = self.financial_market_snapshot(limit)?;
        output.extend(self.financial_ticker_snapshot(limit)?);
        for bytes in &frames {
            if let Some(private) = select(bytes, |kind| {
                !ticker_section(kind) && kind != section::MARKETS && kind != section::STATUS
            })? {
                output.push(private.into());
            }
        }
        // The old references remain until the corresponding private rows have left.
        self.published_financial = self.financial.clone();
        if self.sync_financial_filter() {
            output.extend(self.financial_ticker_snapshot(limit)?);
        }
        if let Some(filter) = &mut self.filter {
            filter.out.take_pending();
        }
        for bytes in &frames {
            if let Some(status) = select(bytes, |kind| kind == section::STATUS)? {
                output.push(status.into());
            }
        }
        Ok(output)
    }
}
