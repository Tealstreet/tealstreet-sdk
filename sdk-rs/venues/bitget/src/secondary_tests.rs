use super::*;
use safe_cex_engine::signing::Provider;
use safe_cex_engine::{CredentialSlot, Query};

pub(crate) fn setup() -> (BitgetFutures, Provider) {
    let (provider, ids) = safe_cex_crypto::signer::LocalSigner::from_json(r#"{"primary":{"key":"primary-key","secret":"AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=","passphrase":"primary-pass"},"secondary":{"key":"read-key","secret":"AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=","passphrase":"read-pass"}}"#.into()).unwrap();
    (
        BitgetFutures::new(Options {
            credentials: ids.primary,
            read_credentials: ids.secondary,
            ..Default::default()
        }),
        std::sync::Arc::new(provider),
    )
}

pub(crate) fn headers(
    venue: BitgetFutures,
    provider: Provider,
    method: Method,
    path: &str,
) -> Vec<(&'static str, String)> {
    struct Probe {
        venue: BitgetFutures,
        method: Method,
        path: String,
    }
    impl Venue for Probe {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy};
            use safe_cex_engine::request_context::{PreflightPolicy, PreflightPurpose};
            let context = if self.method == Method::Get {
                let (credential, _) = self.venue.read_selection(&self.path);
                cx.admit_caller_read_root(credential, None).unwrap()
            } else {
                let credential = Credential {
                    slot: CredentialSlot::Primary,
                    reference: self
                        .venue
                        .options
                        .credentials
                        .as_ref()
                        .unwrap()
                        .credential
                        .clone(),
                };
                let preflight = cx
                    .begin_preflight(PreflightPolicy {
                        purpose: PreflightPurpose::WriteAfterReads,
                        read_credential: None,
                        write_credential: Some(credential.clone()),
                    })
                    .unwrap();
                let (plan, slot) = cx
                    .admit_serial_plan(
                        &preflight,
                        SerialCall {
                            operation: Operation::Mutation,
                            timeout_ms: 20_000,
                            attempts: 1,
                        },
                        SerialPolicy {
                            credential: Some(credential),
                            pacing: None,
                            refusals_complete: false,
                        },
                    )
                    .unwrap();
                cx.plan_leg_context(&plan, &slot).unwrap()
            };
            let identity = self
                .venue
                .signing_identity(self.method, &self.path)
                .cloned();
            let pending = context.clone();
            cx.request_signed_managed(
                context,
                None,
                self.method,
                format!("{}{}", paths::REST, self.path),
                None,
                Box::new(sign::Http::new(identity, self.path.clone())),
                pending,
            )
            .unwrap_or_else(|(_, error)| panic!("managed header probe: {error:?}"));
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<
            safe_cex_engine::request_context::ReconciliationData,
            safe_cex_engine::request_context::Error,
        > {
            safe_cex_engine::request_context::ReconciliationData::new(self.path.clone())
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        venue,
        method,
        path: path.into(),
    });
    engine.set_signer(Some(provider)).unwrap();
    engine.start(1_700_000_000_000.);
    let command = if method == Method::Get {
        Command::Query {
            command: 1,
            query: Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }
    } else {
        Command::PlaceOrders {
            command: 1,
            orders: vec![],
        }
    };
    engine
        .handle_command(1_700_000_000_000., command, Some(60_000))
        .unwrap();
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1_700_000_000_000.);
    actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::Http(http) => Some(http.headers.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no headers for {method:?}: {actions:?}"))
}

#[test]
fn secondary_get_keeps_primary_mutations() {
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let (venue, provider) = setup();
        let headers = headers(venue, provider, method, "/secondary-test");
        let (key, pass, material) = if method == Method::Get {
            (
                "read-key",
                "read-pass",
                "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=",
            )
        } else {
            (
                "primary-key",
                "primary-pass",
                "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
            )
        };
        assert_eq!(
            headers,
            sign::oracle::headers(
                key,
                material,
                pass,
                1_700_000_000_000.,
                method.as_str(),
                "/secondary-test",
                ""
            )
        );
        assert!(headers.contains(&("ACCESS-KEY", key.into())));
        assert!(headers.contains(&("ACCESS-PASSPHRASE", pass.into())));
    }
}
