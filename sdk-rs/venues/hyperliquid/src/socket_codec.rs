//! Native topic projection and wire encoding; the shared core owns transport.
use super::*;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, FeedPolicy, Interest, Invalidation,
    LifecycleEvent, RetireReason, TrafficPolicy,
};

#[derive(Serialize)]
struct PublicTopic<'a> {
    r#type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dex: Option<&'a str>,
}

#[derive(Serialize)]
struct Send<T> {
    method: &'static str,
    subscription: T,
}

#[derive(serde::Deserialize)]
struct EncodedTopic<'a> {
    #[serde(borrow)]
    subscription: DepthOrder<'a>,
}

#[derive(serde::Deserialize)]
struct DepthOrder<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(rename = "nSigFigs", default)]
    figures: Option<u8>,
}

impl Hyperliquid {
    pub(super) fn socket_refused(cx: &mut Cx<'_, Pending>, error: SocketError) {
        cx.notice(
            safe_cex_engine::Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("Managed Hyperliquid socket admission refused: {error:?}"),
        );
    }

    pub(super) fn desired_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        policy: PublicFeedPolicy,
    ) -> Vec<Interest> {
        let mut interests = self.private_socket_interests(store);
        if policy != PublicFeedPolicy::Direct {
            return interests;
        }
        let add = |values: &mut Vec<Interest>, kind, coin, interval, dex| {
            values.push(Interest::from_wire_args(
                serde_json::to_string(&Send {
                    method: "subscribe",
                    subscription: PublicTopic {
                        r#type: kind,
                        coin,
                        interval,
                        dex,
                    },
                })
                .expect("native public topic"),
            ));
        };
        add(&mut interests, "allMids", None, None, None);
        if !self.spot {
            add(&mut interests, "fastAssetCtxs", None, None, None);
        }
        if !self.options.public_data_only && self.user().is_some() {
            for dex in &self.dex_names {
                add(&mut interests, "allMids", None, None, Some(dex.as_str()));
            }
        }
        for symbol in self.books.keys() {
            let Some(market) = store.market(symbol) else {
                continue;
            };
            if self.spot {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Depth<'a> {
                    r#type: &'static str,
                    coin: &'a str,
                    n_sig_figs: Option<u8>,
                }
                for n_sig_figs in [None, Some(2), Some(3), Some(4), Some(5)] {
                    interests.push(Interest::from_wire_args(
                        serde_json::to_string(&Send {
                            method: "subscribe",
                            subscription: Depth {
                                r#type: "l2Book",
                                coin: &market.id,
                                n_sig_figs,
                            },
                        })
                        .expect("native spot depth topic"),
                    ));
                }
            } else {
                for kind in ["l2Book", "bbo"] {
                    add(&mut interests, kind, Some(market.id.as_str()), None, None);
                }
            }
        }
        for (symbol, interval) in self.candles.keys() {
            if let Some(market) = store.market(symbol) {
                add(
                    &mut interests,
                    "candle",
                    Some(market.id.as_str()),
                    Some(interval.as_str()),
                    None,
                );
            }
        }
        interests
    }

    pub(super) fn managed_connect(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        let feed = if let Some(feed) = self.connection.feed {
            feed
        } else {
            let feed = cx.register_socket_feed(FeedPolicy {
                role: if self.options.public_data_only {
                    safe_cex_engine::SocketChannel::Public
                } else {
                    safe_cex_engine::SocketChannel::Private
                },
                supervision: Config {
                    idle_timeout_ms: 45_000.,
                    ping_interval_ms: Some(15_000.),
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
                subscription_interval_ms: 0.,
            })?;
            self.connection.feed = Some(feed);
            feed
        };
        if self.options.public_data_only && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        cx.connect_socket(feed, safe_cex_engine::SocketSpec::new(self.ws()))?;
        Ok(())
    }

    pub(super) fn schedule_socket_refresh(&mut self, cx: &mut Cx<'_, Pending>, mode_ready: bool) {
        let private = if self.connection.open {
            self.private_socket_interests(cx.store)
        } else {
            Vec::new()
        };
        if private.is_empty() {
            self.finish_socket_refresh(cx, mode_ready);
            if self.connection.open {
                self.propose_socket_interests(cx);
            }
            return;
        }
        let Some(feed) = self.connection.feed else {
            return;
        };
        let desired = self.desired_socket_interests(cx.store, cx.public_feed_policy());
        let required: Vec<_> = private
            .iter()
            .map(|interest| interest.key.as_ref())
            .collect();
        let epoch = cx.socket_epoch(feed);
        if let Err(error) = cx.set_socket_interests_then(
            feed,
            desired,
            &required,
            Pending::SocketRefresh { mode_ready },
        ) {
            Self::socket_refused(cx, error);
            if error == SocketError::Capacity && epoch.is_some() && cx.socket_epoch(feed) == epoch {
                cx.retire_socket(feed, RetireReason::Closed);
            }
        }
    }

    pub(super) fn finish_socket_refresh(&mut self, cx: &mut Cx<'_, Pending>, mode_ready: bool) {
        if !mode_ready
            && self.account.started
            && self.user().is_some()
            && !self.options.public_data_only
        {
            self.refresh_account(cx);
        }
        self.finish_mode_refresh(cx);
    }

    pub(super) fn managed_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                if self.connection.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                self.connection.open = true;
                if cx.public_feed_policy() == PublicFeedPolicy::Direct {
                    for ladder in self.books.values_mut() {
                        ladder.spot_depth = Default::default();
                        ladder.depth_time = None;
                        ladder.bbo = None;
                        ladder.book.clear();
                    }
                }
                self.schedule_socket_refresh(cx, false);
            }
            LifecycleEvent::Retired { epoch, .. } => {
                if self.connection.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                self.connection.open = false;
                for ladder in self.books.values_mut() {
                    ladder.spot_depth = Default::default();
                    ladder.depth_time = None;
                    ladder.book.clear();
                }
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "{\"method\":\"ping\"}"),
            LifecycleEvent::Reconnect(_) => self.managed_connect(cx)?,
            LifecycleEvent::InterestsRefused { feed, error, .. } => {
                if error != SocketError::Capacity || self.connection.feed != Some(feed) {
                    return Err(error);
                }
                Self::socket_refused(cx, error);
                cx.retire_socket(feed, RetireReason::Closed);
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
        if self.connection.feed != Some(feed) {
            return Err(SocketError::Stale);
        }
        let desired = self.desired_socket_interests(store, PublicFeedPolicy::Direct);
        let ranks: HashMap<_, _> = desired
            .iter()
            .enumerate()
            .map(|(index, interest)| (interest.key.as_ref(), index))
            .collect();
        let mut ordered: Vec<_> = interests
            .iter()
            .map(|interest| {
                let value: EncodedTopic<'_> = serde_json::from_str(&interest.wire_args)
                    .map_err(|_| SocketError::InvalidInterest)?;
                let figures = value.subscription.figures.unwrap_or(0);
                let book_order = usize::from(value.subscription.kind == "bbo");
                Ok((
                    interest,
                    ranks
                        .get(interest.key.as_ref())
                        .copied()
                        .unwrap_or(usize::MAX),
                    book_order,
                    figures,
                ))
            })
            .collect::<Result<_, SocketError>>()?;
        ordered.sort_by_key(|(_, rank, book_order, figures)| (*rank, *book_order, *figures));
        for (interest, _, _, _) in ordered {
            let text = if subscribe {
                interest.wire_args.to_string()
            } else {
                interest.wire_args.replacen(
                    "\"method\":\"subscribe\"",
                    "\"method\":\"unsubscribe\"",
                    1,
                )
            };
            emit(EncodedBatch {
                text,
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
}
