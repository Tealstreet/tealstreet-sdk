//! Inspect real retained handles without exposing a production context counter.
use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Admission, Error};
use safe_cex_engine::{Action, Command, Event, Notice, SocketId, TimerId, Venue};

struct ObservedHistory {
    backpack: Backpack,
    retired: Vec<Context>,
    root: Option<Context>,
    released: usize,
}
impl ObservedHistory {
    fn assert_retired(&self, cx: &Cx<'_, Pending>) {
        for context in &self.retired {
            assert_eq!(cx.validate_http_context(context), Err(Error::Revoked));
        }
    }
}
impl Venue for ObservedHistory {
    type Pending = Pending;
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.assert_retired(cx);
        let Command::Query { command, query } = command else {
            panic!("history query only");
        };
        self.backpack.history_command(cx, command, query);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.backpack.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let Pending::History(request) = &pending else {
            panic!("history continuation only");
        };
        let leg = request.leg.as_ref().unwrap().context().clone();
        self.backpack.on_response(cx, pending, response);
        self.retired.push(leg);
        self.assert_retired(cx);
        let root = self.root.as_ref().unwrap();
        if self.retired.len() < 50 {
            assert!(cx.validate_read_root_context(root).is_ok());
        } else {
            assert_eq!(cx.validate_http_context(root), Err(Error::Revoked));
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, error: &str) {
        panic!("unexpected failure: {error}");
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::Mutable(|this, pending: &mut Pending, _| {
            let Pending::History(request) = pending else {
                panic!("history release only");
            };
            let context = request.root.context();
            if let Some(root) = &this.root {
                assert_eq!(root, context, "every page keeps the original root");
            } else {
                this.root = Some(context.clone());
            }
            this.released += 1;
            assert_eq!(this.retired.len() + 1, this.released);
            Ok(Admission::Unscheduled)
        })
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn fifty_pages_retire_every_exact_leg_and_the_original_root() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::KeyEncoding;
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: None,
        secondary: Some(LocalIdentity {
            key: Secret::new("read-key".into()),
            secret: Secret::new(STANDARD.encode([9; 32])),
            secret_encoding: KeyEncoding::Base64,
            ..Default::default()
        }),
    })
    .unwrap();
    let backpack = Backpack::new(crate::Options {
        public_data_only: true,
        read_identity: identities.secondary.clone(),
        read_credentials_status: Some(Ok(())),
        ..Default::default()
    });
    let mut engine = safe_cex_engine::test_support::seeded_engine(ObservedHistory {
        backpack,
        retired: vec![],
        root: None,
        released: 0,
    });
    engine
        .set_signer(Some(std::sync::Arc::new(provider)))
        .unwrap();
    engine.set_identities(identities).unwrap();
    engine
        .handle_command(
            2000.,
            Command::Query {
                command: 71,
                query: Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
            },
            Some(50_000),
        )
        .unwrap();
    for page in 0..50 {
        let now = 2001. + f64::from(page);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, now);
        let requests: Vec<_> = actions
            .into_iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                Action::Notice(Notice::CommandFailed { error, .. }) => {
                    panic!("early failure: {error:?}");
                }
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1, "one released leg per page");
        let packet = &requests[0];
        assert_eq!(
            packet
                .headers
                .iter()
                .find(|(name, _)| *name == "X-API-Key")
                .unwrap()
                .1,
            "read-key"
        );
        if page > 0 {
            assert!(packet.url.contains(&format!("offset={page}")));
        }
        let body = format!(
            r#"[{{"orderId":"123","symbol":"UNLISTED_PERP","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":{page},"timestamp":"2025-12-15T13:28:00Z"}}]"#
        );
        engine.handle(
            now,
            Event::HttpResponse {
                request: packet.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
    }
    assert_eq!(engine.venue().released, 50);
    assert_eq!(engine.venue().retired.len(), 50);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 71, result }) if result == "[]"
            ))
            .count(),
        1
    );
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.code == "history.incomplete"))
            .count(),
        1
    );
    assert!(!actions.iter().any(|action| match action {
        Action::Http(_) | Action::Notice(Notice::CommandFailed { command: 71, .. }) => true,
        Action::ManagedHttp(action) => matches!(
            **action,
            safe_cex_engine::request_context::HttpAction::Prepare { .. }
                | safe_cex_engine::request_context::HttpAction::Bind { .. }
        ),
        _ => false,
    }));
}
