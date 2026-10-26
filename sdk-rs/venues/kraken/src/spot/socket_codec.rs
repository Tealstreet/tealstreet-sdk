//! Spot token and subscriptions are scoped to the engine's physical epoch.
use super::*;
use safe_cex_engine::socket_lifecycle::{
    Control, EncodedBatch, FeedId, Interest, LifecycleEvent, SocketError,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct Topic {
    channel: String,
    symbol: Vec<String>,
    interval: Option<u32>,
}
#[derive(serde::Serialize)]
struct Params<'a> {
    channel: &'a str,
    symbol: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_trigger: Option<&'static str>,
}
#[derive(serde::Serialize)]
struct Frame<'a> {
    method: &'a str,
    params: Params<'a>,
}
impl KrakenSpot {
    fn private_feed(&self, feed: FeedId) -> Result<bool, SocketError> {
        if self.public.feed == Some(feed) {
            Ok(false)
        } else if self.account.connection.feed == Some(feed) {
            Ok(true)
        } else {
            Err(SocketError::Stale)
        }
    }
    pub(super) fn managed_connect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        private: bool,
    ) -> Result<(), SocketError> {
        if !private && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        if self.public.feed.is_none() && cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.feed =
                Some(cx.register_socket_feed(crate::socket_codec::policy(false, false))?);
        }
        if self.account.connection.feed.is_none()
            && self.signer.is_some()
            && !self.options.public_data_only
        {
            self.account.connection.feed =
                Some(cx.register_socket_feed(crate::socket_codec::policy(true, false))?);
        }
        let feed = if private {
            self.account.connection.feed
        } else {
            self.public.feed
        }
        .ok_or(SocketError::NotPermitted)?;
        if !private {
            cx.set_socket_interests(feed, self.public_interests(cx.store))?;
        }
        let epoch = cx.connect_socket(
            feed,
            if private {
                "wss://ws-auth.kraken.com/v2"
            } else {
                "wss://ws.kraken.com/v2"
            },
        )?;
        let connection = if private {
            &mut self.account.connection
        } else {
            &mut self.public
        };
        connection.socket = Some(epoch.socket());
        connection.open = false;
        Ok(())
    }
    pub(super) fn socket_error(&self, cx: &mut Cx<'_, Pending>, private: bool, error: SocketError) {
        if let Some(feed) = if private {
            self.account.connection.feed
        } else {
            self.public.feed
        } {
            cx.stop_socket_feed(feed);
        }
        cx.notice(
            Level::Error,
            code::STREAM_UNAVAILABLE,
            format!("Kraken Spot socket admission failed: {error:?}"),
        );
    }
    fn interest(&self, channel: &str, symbols: &[&str], interval: Option<u32>) -> Interest {
        let symbol = symbols
            .iter()
            .map(|s| {
                self.ws_symbols
                    .get(*s)
                    .cloned()
                    .unwrap_or_else(|| (*s).into())
            })
            .collect();
        Interest::from_wire_args(
            serde_json::to_string(&Topic {
                channel: channel.into(),
                symbol,
                interval,
            })
            .unwrap(),
        )
    }
    fn public_interests(&self, store: &safe_cex_engine::Store) -> Vec<Interest> {
        let symbols: Vec<_> = store
            .markets()
            .iter()
            .filter(|m| self.ticker_symbols.is_empty() || self.ticker_symbols.contains(&m.symbol))
            .map(|m| m.symbol.as_str())
            .collect();
        let mut interests = vec![];
        for batch in symbols.chunks(50) {
            interests.push(self.interest("ticker", batch, None));
        }
        let mut books: Vec<_> = self.books.keys().collect();
        books.sort();
        for symbol in books {
            interests.push(self.interest("book", &[symbol], None));
        }
        let mut candles: Vec<_> = self
            .candles
            .keys()
            .filter_map(|(s, tf)| interval(*tf).map(|i| (s, i)))
            .collect();
        candles.sort();
        candles.dedup();
        for (symbol, i) in candles {
            interests.push(self.interest("ohlc", &[symbol], Some(i)));
        }
        interests
    }
    pub(super) fn sync_public_interests(&self, cx: &mut Cx<'_, Pending>) {
        if let Some(feed) = self.public.feed
            && let Err(error) = cx.set_socket_interests(feed, self.public_interests(cx.store))
        {
            self.socket_error(cx, false, error);
        }
    }
    pub(super) fn repair_book(&self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(feed) = self.public.feed
            && let Err(error) = cx.repair_socket_interests(
                feed,
                vec![self.interest("book", &[symbol], None).key.to_string()],
            )
        {
            self.socket_error(cx, false, error);
        }
    }
    pub(super) fn managed_control(
        &self,
        feed: FeedId,
        _authenticated: bool,
        text: &str,
    ) -> Control {
        if self.private_feed(feed).is_err() {
            return Control::Other;
        }
        match serde_json::from_str::<Ws<'_>>(text) {
            Ok(msg) if msg.method == "pong" => Control::Pong,
            Ok(_) => Control::Traffic,
            Err(_) => Control::Other,
        }
    }
    pub(super) fn managed_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                if self.private_feed(epoch.feed())? {
                    cx.set_socket_interests(epoch.feed(), vec![])?;
                }
                self.on_open(cx, epoch.socket());
            }
            LifecycleEvent::Retired { epoch, .. } => {
                self.private_feed(epoch.feed())?;
                self.on_close(cx, epoch.socket(), 1006);
            }
            LifecycleEvent::Reconnect(feed) => {
                self.managed_connect(cx, self.private_feed(feed)?)?
            }
            LifecycleEvent::Ping(epoch) => {
                self.private_feed(epoch.feed())?;
                cx.send(epoch.socket(), r#"{"method":"ping"}"#);
            }
            LifecycleEvent::InterestsRefused { feed, error, .. } => {
                self.socket_error(cx, self.private_feed(feed)?, error)
            }
            _ => (),
        }
        Ok(())
    }
    pub(super) fn managed_encode(
        &self,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let private = self.private_feed(feed)?;
        if private {
            return if items.is_empty() {
                Ok(())
            } else {
                Err(SocketError::InvalidInterest)
            };
        }
        let mut ordered: Vec<_> = items.iter().collect();
        ordered.sort_by(|a, b| {
            let rank = |s: &str| {
                if s.contains("\"channel\":\"ticker\"") {
                    0
                } else if s.contains("\"channel\":\"book\"") {
                    1
                } else {
                    2
                }
            };
            rank(&a.wire_args)
                .cmp(&rank(&b.wire_args))
                .then(a.key.cmp(&b.key))
        });
        for item in ordered {
            let topic: Topic =
                serde_json::from_str(&item.wire_args).map_err(|_| SocketError::InvalidInterest)?;
            let channel = topic.channel.as_str();
            let text = serde_json::to_string(&Frame {
                method: if subscribe {
                    "subscribe"
                } else {
                    "unsubscribe"
                },
                params: Params {
                    channel,
                    symbol: &topic.symbol,
                    depth: (channel == "book").then_some(100),
                    interval: topic.interval,
                    event_trigger: (channel == "ticker").then_some("trades"),
                },
            })
            .map_err(|_| SocketError::InvalidInterest)?;
            emit(EncodedBatch {
                text,
                covered_keys: vec![item.key.clone()],
            })?;
        }
        Ok(())
    }
}
