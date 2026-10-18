use super::*;
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities},
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    journal::JournalFolder,
    runtime::{ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use std::{pin::Pin, rc::Rc};

struct Runtime;
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        901.25
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected spawn")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {
        panic!("unexpected console")
    }
    fn unique_id(&self) -> String {
        panic!("unexpected id")
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        panic!("unexpected random")
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {
        panic!("unexpected exception")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn class_name(&self) -> &'static str {
        "KrakenExchange"
    }
}
struct Folder {
    expected_base: *const BaseExchange,
    calls: RefCell<Vec<Value>>,
    error: RefCell<Option<ExchangeError>>,
    records: RefCell<Option<Vec<Value>>>,
}
impl JournalFolder for Folder {
    fn build_folded_position_records<'a>(
        &'a self,
        venue: &'a dyn Exchange,
        account: &'a str,
        from: f64,
        to: f64,
        symbol_scoped: bool,
        settle: Box<dyn Fn(&str) -> String + 'a>,
    ) -> ExchangeFuture<'a, Vec<Value>> {
        self.calls.borrow_mut().push(
            json!({"account":account,"from":number(from),"to":number(to),
            "symbolScoped":symbol_scoped,"sameVenue":std::ptr::eq(venue.base(),self.expected_base),
            "settles":[settle("PF_XBTUSD"),settle("EMPTY"),settle("MISSING")]}),
        );
        Box::pin(async move {
            if let Some(error) = self.error.borrow_mut().take() {
                return Err(error);
            }
            Ok(self.records.borrow_mut().take().expect("one folder call"))
        })
    }
}
#[test]
fn historical_positions_live_oracle() {
    let source = &oracle()["histories"];
    assert_eq!(source.as_array().unwrap().len(), 10, "HISTORY source cases");
    let mut comparisons = 0;
    for index in 0..10 {
        let case = &source[index];
        let store = Rc::new(DefaultStore::new());
        let markets: Vec<Market>=serde_json::from_value(json!([
            {"id":"A","symbol":"PF_XBTUSD","base":"XBT","quote":"USD","active":true,
             "precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":9},"leverage":{"min":1,"max":5}}},
            {"id":"B","symbol":"EMPTY","base":"XBT","quote":"","active":true,
             "precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":9},"leverage":{"min":1,"max":5}}}
        ])).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let mut capabilities = ExchangeCapabilities::default();
        if index != 8 {
            capabilities.fetch_historical_fills =
                CapabilityBinding::implemented(|_, _| Box::pin(async { Ok(vec![]) }));
        }
        let venue = Venue {
            base: BaseExchange::new(
                safe_cex_types::ExchangeName::Kraken,
                serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
                store,
                Rc::new(Runtime),
                ExchangeSettings::default(),
            ),
            capabilities,
        };
        let records = vec![
            json!({"id":"record-A","symbol":"PF_XBTUSD"}),
            json!({"id":"record-B","symbol":"PF_ETHUSD"}),
        ];
        let record_pointer = records.as_ptr();
        let mut error = ExchangeError::new("folder failure");
        error.metadata.store_subscriber_ids = vec![71, 83];
        let error_pointer = error.metadata.as_ref() as *const _;
        let folder = Folder {
            expected_base: &venue.base,
            calls: RefCell::new(vec![]),
            error: RefCell::new((index == 9).then_some(error)),
            records: RefCell::new(Some(records)),
        };
        let result = complete(reads::fetch_historical_positions(
            &venue,
            &folder,
            serde_json::from_str(case["raw"].as_str().unwrap()).unwrap(),
        ));
        assert_eq!(
            folder.calls.borrow().len(),
            usize::from(index != 8),
            "HISTORY native folder call count"
        );
        assert_eq!(
            *folder.calls.borrow(),
            case["calls"].as_array().unwrap().clone(),
            "HISTORY native fold arguments"
        );
        match result {
            Ok(records) => {
                assert!(index < 8, "HISTORY native success case");
                assert_eq!(records.len(), 2, "HISTORY native record count");
                assert_eq!(
                    records,
                    case["result"].as_array().unwrap().clone(),
                    "HISTORY native records"
                );
                assert_eq!(
                    records.as_ptr(),
                    record_pointer,
                    "HISTORY record allocation retained"
                );
                assert_eq!(case["sameResult"], true, "HISTORY source record identity");
            }
            Err(error) => {
                assert!(index >= 8, "HISTORY native error case");
                assert_eq!(
                    error.message,
                    case["error"].as_str().unwrap(),
                    "HISTORY native error message"
                );
                if index == 9 {
                    assert!(
                        std::ptr::eq(error.metadata.as_ref(), error_pointer),
                        "HISTORY error allocation retained"
                    );
                    assert_eq!(
                        error.metadata.store_subscriber_ids,
                        vec![71, 83],
                        "HISTORY subscriber identity"
                    );
                    assert_eq!(case["sameError"], true, "HISTORY source error identity");
                }
            }
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 10, "HISTORY completed comparisons");
}

#[test]
fn historical_options_chosen_boundary() {
    let case = &oracle()["historyBoundary"];
    assert_eq!(
        case["result"]["account"],
        number(17.0),
        "HISTORY source accepts numeric account"
    );
    let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
    // The same registered decoder is exercised by the production call below via an inert venue.
    let venue = Venue {
        base: BaseExchange::new(
            safe_cex_types::ExchangeName::Kraken,
            serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            Rc::new(DefaultStore::new()),
            Rc::new(Runtime),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities::default(),
    };
    let folder = Folder {
        expected_base: &venue.base,
        calls: RefCell::new(vec![]),
        error: RefCell::new(None),
        records: RefCell::new(None),
    };
    let error = complete(reads::fetch_historical_positions(&venue, &folder, input)).unwrap_err();
    assert!(
        error.message.contains("string"),
        "HISTORY chosen string account refusal"
    );
    assert_eq!(
        folder.calls.borrow().len(),
        0,
        "HISTORY refused before fold"
    );
}
