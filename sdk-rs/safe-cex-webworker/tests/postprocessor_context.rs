use safe_cex_webworker::postprocessor_context::{
    BasePostprocessor, BasePostprocessorContext, ExchangeDependentPostprocessor,
    ExchangeDependentPostprocessorContext,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default, Deserialize)]
struct Options {
    buy: f64,
    sell: f64,
}
#[derive(Default)]
struct Exchange {
    marker: f64,
}
type OptionsRef = Rc<RefCell<Options>>;
type ExchangeRef = Rc<RefCell<Exchange>>;
#[derive(Default)]
struct Store {
    value: f64,
    name: Option<Vec<u16>>,
    exchange: Option<ExchangeRef>,
}
struct Independent {
    context: BasePostprocessorContext<Vec<u16>, String, OptionsRef>,
}
impl BasePostprocessor for Independent {
    type AccountName = Vec<u16>;
    type ExchangeName = String;
    type Options = OptionsRef;
    type Store = Store;
    fn context(&self) -> &BasePostprocessorContext<Vec<u16>, String, OptionsRef> {
        &self.context
    }
    fn process(&mut self, store: &mut Store) {
        store.value = self.context.options.borrow().buy;
        store.name = Some(self.context.account_name.clone());
    }
}
struct Dependent {
    context: ExchangeDependentPostprocessorContext<Vec<u16>, String, OptionsRef, ExchangeRef>,
}
impl ExchangeDependentPostprocessor for Dependent {
    type AccountName = Vec<u16>;
    type ExchangeName = String;
    type Options = OptionsRef;
    type Exchange = ExchangeRef;
    type Store = Store;
    fn context(
        &self,
    ) -> &ExchangeDependentPostprocessorContext<Vec<u16>, String, OptionsRef, ExchangeRef> {
        &self.context
    }
    fn process(&mut self, store: &mut Store) {
        store.value = self.context.options.borrow().buy;
        store.exchange = Some(self.context.exchange.clone());
    }
}
fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}
const NAMES: [&[u16]; 4] = [
    &[97, 99, 99, 111, 117, 110, 116],
    &[0x00e9, 0xd83e, 0xde99],
    &[],
    &[0xd800],
];

#[test]
fn abstract_contexts_preserve_inputs_and_dispatch_required_overrides() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/postprocessor_context_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!source["architecture"].as_str().unwrap().is_empty());
    let declarations = source["declarations"].as_array().unwrap();
    assert_eq!(declarations.len(), 2, "CONTEXT source declaration count");
    for (index, (name, fields)) in [
        (
            "BasePostprocessor",
            vec!["_accountName", "_exchangeName", "_options"],
        ),
        (
            "ExchangeDependentPostprocessor",
            vec!["_accountName", "_exchangeName", "_options", "_exchange"],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            declarations[index]["name"], name,
            "CONTEXT source declaration identity"
        );
        assert_eq!(
            declarations[index]["assignments"],
            json!(fields),
            "CONTEXT source field inventory"
        );
        assert_eq!(
            declarations[index]["abstractProcess"], true,
            "CONTEXT source abstract method"
        );
    }
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "CONTEXT source case count");
    let mut comparisons = 0;
    for (kind_index, kind) in ["independent", "dependent"].into_iter().enumerate() {
        for (index, name) in NAMES.into_iter().enumerate() {
            let row = &rows[kind_index * 4 + index];
            assert_eq!(row["kind"], kind, "CONTEXT source case kind");
            assert_eq!(row["index"], index, "CONTEXT source fixed case index");
            let options: Options = serde_json::from_str("{\"buy\":1.25,\"sell\":7.5}").unwrap();
            let options = Rc::new(RefCell::new(options));
            let exchange = Rc::new(RefCell::new(Exchange { marker: 53.0 }));
            let mut independent = Independent {
                context: BasePostprocessorContext::new(
                    name.to_vec(),
                    "bybit".to_owned(),
                    options.clone(),
                ),
            };
            let mut dependent = Dependent {
                context: ExchangeDependentPostprocessorContext::new(
                    name.to_vec(),
                    "bybit".to_owned(),
                    options.clone(),
                    exchange.clone(),
                ),
            };
            let (account, exchange_name, held) = if kind_index == 0 {
                let processor: &dyn BasePostprocessor<
                    AccountName = Vec<u16>,
                    ExchangeName = String,
                    Options = OptionsRef,
                    Store = Store,
                > = &independent;
                let ctx = processor.context();
                (
                    ctx.account_name.clone(),
                    ctx.exchange_name.clone(),
                    ctx.options.clone(),
                )
            } else {
                let processor: &dyn ExchangeDependentPostprocessor<
                    AccountName = Vec<u16>,
                    ExchangeName = String,
                    Options = OptionsRef,
                    Exchange = ExchangeRef,
                    Store = Store,
                > = &dependent;
                let ctx = processor.context();
                (
                    ctx.account_name.clone(),
                    ctx.exchange_name.clone(),
                    ctx.options.clone(),
                )
            };
            // Check retained, decoded native values before any source/profile comparison.
            assert_eq!(
                account, name,
                "CONTEXT native account code units {kind}/{index}"
            );
            assert_eq!(
                exchange_name, "bybit",
                "CONTEXT native exchange name {kind}/{index}"
            );
            assert_eq!(
                held.borrow().buy.to_bits(),
                1.25_f64.to_bits(),
                "CONTEXT native constructor buy {kind}/{index}"
            );
            assert_eq!(
                held.borrow().sell.to_bits(),
                7.5_f64.to_bits(),
                "CONTEXT native constructor sell {kind}/{index}"
            );
            assert!(
                Rc::ptr_eq(&held, &options),
                "CONTEXT native option identity {kind}/{index}"
            );
            if kind_index == 1 {
                assert!(
                    Rc::ptr_eq(&dependent.context.exchange, &exchange),
                    "CONTEXT native exchange identity"
                );
            }
            let before = json!({"account":account,"exchangeName":exchange_name,"buy":bits(held.borrow().buy),"sell":bits(held.borrow().sell),"optionsSame":true,"exchangeSame":if kind_index==1{Some(true)}else{None}});
            options.borrow_mut().buy = -0.0;
            options.borrow_mut().sell = 99.0;
            exchange.borrow_mut().marker = 71.0;
            let mut store = Store::default();
            if kind_index == 0 {
                let processor: &mut dyn BasePostprocessor<
                    AccountName = Vec<u16>,
                    ExchangeName = String,
                    Options = OptionsRef,
                    Store = Store,
                > = &mut independent;
                processor.process(&mut store);
                assert_eq!(
                    store.name.as_deref(),
                    Some(name),
                    "CONTEXT native override account"
                );
            } else {
                let processor: &mut dyn ExchangeDependentPostprocessor<
                    AccountName = Vec<u16>,
                    ExchangeName = String,
                    Options = OptionsRef,
                    Exchange = ExchangeRef,
                    Store = Store,
                > = &mut dependent;
                processor.process(&mut store);
                assert!(
                    Rc::ptr_eq(store.exchange.as_ref().unwrap(), &exchange),
                    "CONTEXT native override exchange"
                );
                assert_eq!(
                    dependent.context.exchange.borrow().marker.to_bits(),
                    71.0_f64.to_bits(),
                    "CONTEXT native exchange mutation"
                );
            }
            assert_eq!(
                held.borrow().buy.to_bits(),
                (-0.0_f64).to_bits(),
                "CONTEXT native live buy"
            );
            assert_eq!(
                held.borrow().sell.to_bits(),
                99.0_f64.to_bits(),
                "CONTEXT native live sell"
            );
            assert_eq!(
                store.value.to_bits(),
                (-0.0_f64).to_bits(),
                "CONTEXT native dispatched value"
            );
            let after = json!({"buy":bits(held.borrow().buy),"sell":bits(held.borrow().sell),"processed":bits(store.value),"exchangeMarker":if kind_index==1{Some(bits(dependent.context.exchange.borrow().marker))}else{None}});
            assert_eq!(
                before, row["before"],
                "CONTEXT source/native retained fields {kind}/{index}"
            );
            assert_eq!(
                after, row["after"],
                "CONTEXT source/native live fields {kind}/{index}"
            );
            assert_eq!(row["processSame"], true, "CONTEXT source override identity");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 8, "CONTEXT native comparison count");
}
