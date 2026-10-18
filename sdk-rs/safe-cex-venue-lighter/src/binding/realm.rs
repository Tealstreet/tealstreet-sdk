//! The realm the ported `LighterExchange` class runs in, over the shared
//! js_runtime scope. Every host family is implemented on `Bound`, a host
//! view of one owner object.
use super::value::{
    Microtasks, js_to_native, native_to_js, to_account, to_js, to_signing, to_thrown,
};
use crate::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_account_cache::AccountCacheHost,
    exchange_account_lookup::AccountLookupHost,
    exchange_batch_placement::BatchPlacementHost,
    exchange_class::{
        ClassConstructor, ExchangeClassContexts, ExchangeClassRealm, ExchangeMethod, ModuleHelper,
    },
    exchange_error_recovery::ExchangeErrorRecoveryHost,
    exchange_historical_fills::HistoricalFillsHost,
    exchange_live_helpers::LiveHelperHost,
    exchange_market_reads::MarketReadHost,
    exchange_ohlcv::OhlcvHost,
    exchange_order_actions::OrderActionHost,
    exchange_order_mapping::OrderMappingHost,
    exchange_order_snapshot::OrderSnapshotHost,
    exchange_positions::PositionReadHost,
    exchange_reconciliation::ReconciliationHost,
    exchange_runtime_initialization::RuntimeInitializationHost,
    exchange_single_placement::SinglePlacementHost,
    exchange_trading_preflight::{TradingCallback, TradingHost},
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use safe_cex_exchange::js_runtime::{
    promise::{Promise, await_js},
    scope::{Scope, ScopeShared},
    value::{Js, Prop, Thrown},
};
use std::rc::Rc;

pub(crate) struct LighterExtra {
    pub(crate) microtasks: Rc<Microtasks>,
    /// Per instance, the BaseExchange members captured before derived fields
    /// run, so `super.member` survives a derived arrow field of the same name.
    pub(crate) supers: std::cell::RefCell<Vec<(Js, Js)>>,
    /// The host's transport registry (TS `setDefaultTransport` target).
    pub(crate) registry: Rc<safe_cex_transport::TransportRegistry>,
}

pub(crate) type Shared = ScopeShared<LighterExtra>;

/// Realm operations over one shared scope; cheap to clone.
#[derive(Clone)]
pub(crate) struct Realm(pub(crate) Rc<Shared>);

/// Host view of one owner object. All host families are implemented on it.
#[derive(Clone)]
pub(crate) struct Bound {
    pub(crate) realm: Realm,
    pub(crate) owner: Js,
}

impl Realm {
    pub(crate) fn bound(&self, owner: &AccountValue) -> Rc<Bound> {
        Rc::new(Bound {
            realm: self.clone(),
            owner: to_js(owner),
        })
    }
    pub(crate) fn scope(&self) -> Scope<LighterExtra> {
        Scope {
            shared: self.0.clone(),
            this: Js::Undefined,
        }
    }
    pub(crate) fn exchange_runtime(&self) -> Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> {
        self.scope().exchange_runtime()
    }
    pub(crate) fn global(&self, name: &str) -> Result<Js, Thrown> {
        self.scope().global(name)
    }
    fn method(&self, owner: Js, body: ExchangeMethod) -> Js {
        let realm = self.clone();
        let meta = body.metadata();
        Js::function(meta.name, move |_, args| {
            let args: Vec<_> = args.iter().map(to_account).collect();
            body.invoke(&realm, to_account(&owner), &args)
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        })
    }
}

/// A captured callee and receiver; calling never re-reads the member.
pub(crate) struct JsCall {
    pub(crate) callee: Js,
    pub(crate) this: Js,
}

impl AccountAccessCall for JsCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        if !self.callee.is_callable() {
            return Err(to_signing(Thrown::type_error(&format!(
                "{} is not a function",
                self.callee.to_display()
            ))));
        }
        self.callee
            .call(self.this.clone(), args.iter().map(to_js).collect())
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn construct(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.callee
            .construct(args.iter().map(to_js).collect(), None)
            .map(|value| to_account(&value))
            .map_err(to_signing)
    }
}

fn is_thenable(value: &Js) -> Result<bool, Thrown> {
    match value {
        Js::Promise(_) => Ok(true),
        Js::Object(_) | Js::Function(_) => Ok(value.get("then")?.is_callable()),
        _ => Ok(false),
    }
}

impl ExchangeAccountAccessHost for Bound {
    fn owner(&self) -> AccountValue {
        to_account(&self.owner)
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        to_js(object)
            .get(key)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn set(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        to_js(object).set(key, to_js(&value)).map_err(to_signing)
    }
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let this = to_js(object);
        let callee = this.get(key).map_err(to_signing)?;
        Ok(Rc::new(JsCall { callee, this }))
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        if let Some(member) = name.strip_prefix("super.") {
            let supers = self.realm.0.extra.supers.borrow();
            let found = supers.iter().find(|(o, _)| o.same_reference(&self.owner));
            let callee = match found {
                Some((_, members)) => members.get(member).map_err(to_signing)?,
                None => Js::Undefined,
            };
            return Ok(Rc::new(JsCall {
                callee,
                this: self.owner.clone(),
            }));
        }
        let callee = self.realm.global(name).map_err(to_signing)?;
        Ok(Rc::new(JsCall {
            callee,
            this: Js::Undefined,
        }))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::object(
            fields.iter().map(|(k, v)| (*k, to_js(v))).collect(),
        )))
    }
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError {
        to_signing(Thrown::new(to_js(&value)))
    }
    fn resolve_return(&self, value: AccountValue, resolver: PromiseResolver<AccountValue>) {
        let js = to_js(&value);
        match is_thenable(&js) {
            Ok(false) => resolver.settle(Ok(value)),
            Err(e) => resolver.settle(Err(to_signing(e))),
            Ok(true) => {
                let runtime = self.realm.exchange_runtime();
                Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        resolver.settle(match await_js(js).await {
                            Ok(v) => Ok(to_account(&v)),
                            Err(e) => Err(to_signing(e)),
                        });
                        Ok(Js::Undefined)
                    }),
                );
            }
        }
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.realm.0.extra.microtasks.clone()
    }
    fn await_value(&self, value: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        Ok(js_to_native(
            &self.realm.exchange_runtime(),
            self.runtime(),
            to_js(&value),
        ))
    }
}

impl TradingHost for Bound {
    fn function(&self, callback: TradingCallback) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::function("", move |_, args| {
            let args: Vec<_> = args.iter().map(to_account).collect();
            callback(&args).map(|v| to_js(&v)).map_err(to_thrown)
        })))
    }
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::array(values.iter().map(to_js).collect())))
    }
    fn index(&self, object: &AccountValue, key: AccountValue) -> SigningResult<AccountValue> {
        let key = to_js(&key).to_js_string().map_err(to_signing)?;
        to_js(object)
            .get_key(&key)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn constant(&self, name: &str) -> SigningResult<AccountValue> {
        self.realm
            .global(name)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn error_value(&self, error: TransactionSigningError) -> AccountValue {
        to_account(&to_thrown(error).value)
    }
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>> {
        to_js(&value)
            .to_js_string()
            .map(|s| s.to_vec())
            .map_err(to_signing)
    }
}

macro_rules! contexts {
    ($($name:ident => $host:ident),* $(,)?) => {
        $(fn $name(&self, owner: AccountValue) -> Rc<dyn $host> {
            self.bound(&owner)
        })*
    };
}

impl ExchangeClassContexts for Realm {
    contexts! {
        access => ExchangeAccountAccessHost,
        lookup => AccountLookupHost,
        cache => AccountCacheHost,
        trading => TradingHost,
        errors => ExchangeErrorRecoveryHost,
        initialization => RuntimeInitializationHost,
        reconciliation => ReconciliationHost,
        markets => MarketReadHost,
        positions => PositionReadHost,
        order_snapshot => OrderSnapshotHost,
        fills => HistoricalFillsHost,
        ohlcv => OhlcvHost,
        order_actions => OrderActionHost,
        single_placement => SinglePlacementHost,
        batch_placement => BatchPlacementHost,
        order_mapping => OrderMappingHost,
        live_helpers => LiveHelperHost,
    }
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.exchange_runtime(), value))
    }
}

impl ExchangeClassRealm for Realm {
    fn install_module_helper(
        &self,
        name: &'static str,
        length: usize,
        helper: ModuleHelper,
    ) -> SigningResult<()> {
        let _ = length;
        let realm = self.clone();
        let function = Js::function(name, move |this, args| {
            let args: Vec<_> = args.iter().map(to_account).collect();
            helper
                .invoke(realm.bound(&to_account(&this)), &args)
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        });
        self.0.globals.set(name, function).map_err(to_signing)
    }
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        body: ClassConstructor,
    ) -> SigningResult<AccountValue> {
        let _ = length;
        let base = to_js(&base);
        let parent = base.get("prototype").map_err(to_signing)?;
        let prototype = Js::object(vec![]);
        if let Js::Object(o) = &prototype {
            *o.proto.borrow_mut() = (!parent.is_nullish()).then_some(parent);
        }
        let realm = self.clone();
        let slot: Rc<std::cell::OnceCell<Js>> = Rc::default();
        let class_slot = slot.clone();
        let class = Js::constructor(name, None, move |target, args| {
            let class = class_slot.get().cloned().unwrap_or(Js::Undefined);
            let args: Vec<_> = args.iter().map(to_account).collect();
            body(&realm, to_account(&class), to_account(&target), &args)
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        });
        class.define("prototype", prototype).map_err(to_signing)?;
        class.set("__proto__", base).map_err(to_signing)?;
        let _ = slot.set(class.clone());
        Ok(to_account(&class))
    }
    fn define_prototype_method(
        &self,
        class: &AccountValue,
        body: ExchangeMethod,
    ) -> SigningResult<()> {
        let prototype = to_js(class).get("prototype").map_err(to_signing)?;
        let realm = self.clone();
        let meta = body.metadata();
        let method = Js::function(meta.name, move |this, args| {
            let args: Vec<_> = args.iter().map(to_account).collect();
            body.invoke(&realm, to_account(&this), &args)
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        });
        prototype.define(meta.name, method).map_err(to_signing)
    }
    fn arrow(&self, owner: &AccountValue, body: ExchangeMethod) -> SigningResult<AccountValue> {
        Ok(to_account(&self.method(to_js(owner), body)))
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        to_js(object).define(key, to_js(&value)).map_err(to_signing)
    }
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue> {
        self.global(name)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn array_literal(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::array(values.iter().map(to_js).collect())))
    }
    fn finish_class_binding(&self, class: &AccountValue) -> SigningResult<()> {
        // js_runtime functions keep their name natively rather than as a property.
        let Js::Function(function) = to_js(class) else {
            return Err(to_signing(Thrown::type_error(
                "class binding is not a function",
            )));
        };
        let name = function.name.clone();
        self.0
            .globals
            .set_key(&name, to_js(class))
            .map_err(to_signing)
    }
    fn construct_base(
        &self,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let base = to_js(&class).super_constructor().map_err(to_signing)?;
        let instance = base
            .construct(args.iter().map(to_js).collect(), Some(&to_js(&new_target)))
            .map_err(to_signing)?;
        let prototype = to_js(&new_target).get("prototype").map_err(to_signing)?;
        if let Js::Object(o) = &instance {
            *o.proto.borrow_mut() = Some(prototype);
        }
        // Own data members only: reading accessors here would run base getters
        // (isPublicDataOnly) while the class is still under construction.
        let members = Js::object(vec![]);
        if let Js::Object(o) = &instance {
            let methods: Vec<(Rc<[u16]>, Js)> = o
                .props
                .borrow()
                .iter()
                .filter_map(|(key, prop)| match prop {
                    Prop::Data(value) if value.is_callable() => Some((key.clone(), value.clone())),
                    _ => None,
                })
                .collect();
            for (key, value) in methods {
                members.define_key(&key, value).map_err(to_signing)?;
            }
        }
        self.0
            .extra
            .supers
            .borrow_mut()
            .push((instance.clone(), members));
        Ok(to_account(&instance))
    }
}
