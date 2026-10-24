//! Native topic projection; shared feeds own transport, pacing and release.
use super::*;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, FeedPolicy, Interest, Invalidation,
    LifecycleEvent, RetireReason, TrafficPolicy,
};

impl TxFlow {
    pub(super) fn socket_refused(cx: &mut Cx<'_, Pending>, error: SocketError) {
        cx.notice(
            Level::Warn,
            code::SUBSCRIPTION_REJECTED,
            format!("Managed TxFlow socket admission refused: {error:?}"),
        );
    }

    fn private_socket_interests(&self) -> Vec<Interest> {
        if self.options.public_data_only {
            return Vec::new();
        }
        let Some(user) = self.user() else {
            return Vec::new();
        };
        ["orderUpdates", "clearinghouseState", "userAssetData"]
            .into_iter()
            .map(|kind| Self::socket_interest(false, kind, None, None, Some(user), None))
            .collect()
    }

    fn socket_interest(
        api: bool,
        kind: &str,
        coin: Option<&str>,
        interval: Option<&str>,
        user: Option<&str>,
        instrument_id: Option<u32>,
    ) -> Interest {
        #[derive(Serialize)]
        struct Frame<'a> {
            method: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            r#type: Option<&'a str>,
            subscription: Sub<'a>,
        }
        Interest::from_wire_args(
            serde_json::to_string(&Frame {
                method: "subscribe",
                r#type: (!api).then_some(kind),
                subscription: Sub {
                    r#type: kind,
                    coin,
                    interval,
                    user,
                    instrument_id,
                },
            })
            .expect("native TxFlow subscription"),
        )
    }

    fn desired_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        policy: PublicFeedPolicy,
        api: bool,
    ) -> Vec<Interest> {
        let mut values = Vec::new();
        if policy == PublicFeedPolicy::Direct {
            if api {
                for (symbol, asset) in &self.assets {
                    let base = store
                        .market(symbol)
                        .map(|m| m.base.as_str())
                        .unwrap_or(symbol);
                    values.push(Self::socket_interest(
                        true,
                        "ticker",
                        Some(base),
                        None,
                        None,
                        Some(*asset),
                    ));
                }
                for (symbol, interval) in self.candles.keys() {
                    if let Some(asset) = self.assets.get(symbol) {
                        values.push(Self::socket_interest(
                            true,
                            "candle",
                            Some(&asset.to_string()),
                            Some(interval.as_str()),
                            None,
                            None,
                        ));
                    }
                }
            } else {
                values.push(Self::socket_interest(
                    false,
                    "allAssetCtxs",
                    None,
                    None,
                    None,
                    None,
                ));
                for symbol in self.books.keys() {
                    if let Some(asset) = self.assets.get(symbol) {
                        values.push(Self::socket_interest(
                            false,
                            "l2Book",
                            Some(&asset.to_string()),
                            None,
                            None,
                            None,
                        ));
                    }
                }
            }
        }
        if !api {
            values.extend(self.private_socket_interests());
        }
        values
    }

    pub(super) fn managed_connect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        api: bool,
    ) -> Result<(), SocketError> {
        let role = if api || self.options.public_data_only {
            SocketChannel::Public
        } else {
            SocketChannel::Private
        };
        let conn = if api { &mut self.api } else { &mut self.node };
        let feed = if let Some(feed) = conn.feed {
            feed
        } else {
            let feed = cx.register_socket_feed(FeedPolicy {
                role: role.clone(),
                supervision: Config {
                    idle_timeout_ms: 30_000.,
                    ping_interval_ms: Some(10_000.),
                    backoff_base_ms: 1_000.,
                    backoff_cap_ms: 16_000.,
                    ..Config::default()
                },
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: ControlPolicy::None,
                invalidation: Invalidation::Required,
                subscription_interval_ms: 150.,
            })?;
            conn.feed = Some(feed);
            feed
        };
        if role == SocketChannel::Public && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        cx.connect_socket(
            feed,
            safe_cex_engine::SocketSpec::new(format!(
                "{}/{}",
                REST_URL.replacen("https:", "wss:", 1),
                if api { "api-ws" } else { "ws" }
            )),
        )?;
        Ok(())
    }

    pub(super) fn propose_socket_interests(&mut self, cx: &mut Cx<'_, Pending>, api: bool) {
        let conn = if api { &self.api } else { &self.node };
        let Some(feed) = conn.feed.filter(|_| conn.open) else {
            return;
        };
        let desired = self.desired_socket_interests(cx.store, cx.public_feed_policy(), api);
        let private = if !api && self.node.refresh_due {
            self.private_socket_interests()
        } else {
            Vec::new()
        };
        let epoch = cx.socket_epoch(feed);
        let result = if private.is_empty() {
            cx.set_socket_interests(feed, desired)
        } else {
            let required: Vec<_> = private
                .iter()
                .map(|interest| interest.key.as_ref())
                .collect();
            cx.set_socket_interests_then(feed, desired, &required, Pending::SocketRefresh)
        };
        if let Err(error) = result {
            Self::socket_refused(cx, error);
            if error == SocketError::Capacity && epoch.is_some() && cx.socket_epoch(feed) == epoch {
                cx.retire_socket(feed, RetireReason::Closed);
            }
        }
    }

    fn clear_socket_books(&mut self, cx: &mut Cx<'_, Pending>) {
        for (symbol, ladder) in &mut self.books {
            ladder.book.clear();
            ladder.time = None;
            if ladder.published {
                cx.book_changed(symbol);
            }
        }
    }

    pub(super) fn managed_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                let api = self.api.feed == Some(epoch.feed());
                if !api && self.node.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                (if api { &mut self.api } else { &mut self.node }).open = true;
                if !api {
                    if cx.public_feed_policy() == PublicFeedPolicy::Direct {
                        self.clear_socket_books(cx);
                    }
                    if !self.options.public_data_only {
                        if self.user().is_none() {
                            self.account_refresh(cx);
                        } else {
                            self.node.refresh_due = true;
                        }
                    }
                }
                self.propose_socket_interests(cx, api);
            }
            LifecycleEvent::Retired { epoch, .. } => {
                let api = self.api.feed == Some(epoch.feed());
                if !api && self.node.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                (if api { &mut self.api } else { &mut self.node }).open = false;
                if !api {
                    self.clear_socket_books(cx);
                    self.account_disconnected(cx);
                }
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "{\"method\":\"ping\"}"),
            LifecycleEvent::Reconnect(feed) => {
                if self.api.feed != Some(feed) && self.node.feed != Some(feed) {
                    return Err(SocketError::Stale);
                }
                self.managed_connect(cx, self.api.feed == Some(feed))?;
            }
            LifecycleEvent::InterestsRefused { feed, error, .. } => {
                Self::socket_refused(cx, error);
                if error == SocketError::Capacity {
                    cx.retire_socket(feed, RetireReason::Closed);
                } else {
                    return Err(error);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn managed_socket_encode(
        &self,
        store: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let api = self.api.feed == Some(feed);
        if !api && self.node.feed != Some(feed) {
            return Err(SocketError::Stale);
        }
        let desired = self.desired_socket_interests(store, PublicFeedPolicy::Direct, api);
        let ranks: HashMap<_, _> = desired
            .iter()
            .enumerate()
            .map(|(rank, interest)| (interest.key.as_ref(), rank))
            .collect();
        let mut ordered: Vec<_> = interests.iter().collect();
        ordered.sort_by_key(|interest| {
            ranks
                .get(interest.key.as_ref())
                .copied()
                .unwrap_or(usize::MAX)
        });
        for interest in ordered {
            emit(EncodedBatch {
                text: if subscribe {
                    interest.wire_args.to_string()
                } else {
                    interest.wire_args.replacen(
                        "\"method\":\"subscribe\"",
                        "\"method\":\"unsubscribe\"",
                        1,
                    )
                },
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
}
