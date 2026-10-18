//! The language operations `LighterPublicWebsocket::define` assembles the class
//! with, and the per-owner host contexts its members run on.
use super::{
    super::{
        ingress,
        realm::Realm,
        value::{to_account, to_js, to_signing, to_thrown},
    },
    host::Socket,
};
use crate::{
    public_ingress::IngressOps,
    sdk_account_api::AccountValue,
    transaction_signing::{SigningResult, TransactionSigningError},
    ws_public_class::PublicSocketHost,
    ws_public_class::{
        PublicMethod, PublicSocketConstructor, PublicSocketContexts, PublicSocketRealm,
    },
    ws_public_cleanup::PublicCleanupHost,
    ws_public_readiness::PublicReadinessHost,
    ws_public_subscriptions::PublicSubscriptionHost,
    ws_public_watchdogs::PublicWatchdogHost,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, to_typed};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};

type AccountOps = IngressOps<AccountValue, TransactionSigningError>;

pub(crate) struct SocketRealm {
    realm: Realm,
    ingress: Rc<AccountOps>,
    /// The BaseWebSocket class `create_derived_class` extended.
    base: RefCell<Js>,
    /// BaseWebSocket.prototype at class creation: the `super` lookup object.
    home_parent: RefCell<Js>,
    this: Weak<SocketRealm>,
}

fn js<T>(result: Result<T, Thrown>) -> SigningResult<T> {
    result.map_err(to_signing)
}

impl SocketRealm {
    pub(super) fn new(realm: Realm) -> Rc<Self> {
        let ingress = Rc::new(account_ops(ingress::ops(&realm)));
        Rc::new_cyclic(|this| Self {
            realm,
            ingress,
            base: RefCell::new(Js::Undefined),
            home_parent: RefCell::new(Js::Undefined),
            this: this.clone(),
        })
    }
    fn me(&self) -> Result<Rc<Self>, Thrown> {
        self.this
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "LighterPublicWebsocket realm was dropped"))
    }
    fn socket_host(&self, owner: AccountValue) -> Rc<Socket> {
        Socket::new(
            self.realm.clone(),
            to_js(&owner),
            self.home_parent.borrow().clone(),
        )
    }
}

impl PublicSocketContexts for SocketRealm {
    fn socket(&self, owner: AccountValue) -> Rc<dyn PublicSocketHost> {
        self.socket_host(owner)
    }
    fn readiness(&self, owner: AccountValue) -> Rc<dyn PublicReadinessHost> {
        self.socket_host(owner)
    }
    fn subscriptions(&self, owner: AccountValue) -> Rc<dyn PublicSubscriptionHost> {
        self.socket_host(owner)
    }
    fn cleanup(&self, owner: AccountValue) -> Rc<dyn PublicCleanupHost> {
        self.socket_host(owner)
    }
    fn watchdogs(&self, owner: AccountValue) -> Rc<dyn PublicWatchdogHost> {
        self.socket_host(owner)
    }
    fn ingress(&self, _owner: AccountValue) -> Rc<AccountOps> {
        self.ingress.clone()
    }
    fn store_tickers(&self, owner: AccountValue) -> SigningResult<Vec<safe_cex_types::Ticker>> {
        let store = js(to_js(&owner).get("parent").and_then(|p| p.get("store")))?;
        js(store.get("tickers").and_then(|t| to_typed(&t)))
    }
    fn parse_rest_order_book_levels(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        js(crate::ws_public::parse_rest_order_book_levels(Some(
            &to_js(value),
        )))
        .map(|value| to_account(&value))
    }
    fn parse_rest_ticker_items(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        js(crate::ws_public::parse_rest_ticker_items(Some(&to_js(
            value,
        ))))
        .map(|value| to_account(&value))
    }
}

impl PublicSocketRealm for SocketRealm {
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue> {
        js(self.realm.global(name)).map(|v| to_account(&v))
    }
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        body: PublicSocketConstructor,
    ) -> SigningResult<AccountValue> {
        let _ = length;
        let base = to_js(&base);
        let parent = js(base.get("prototype"))?;
        let prototype = Js::object(vec![]);
        if let Js::Object(o) = &prototype {
            *o.proto.borrow_mut() = (!parent.is_nullish()).then(|| parent.clone());
        }
        *self.base.borrow_mut() = base;
        *self.home_parent.borrow_mut() = parent;
        let me = js(self.me())?;
        let slot: Rc<OnceCell<Js>> = Rc::default();
        let class_slot = slot.clone();
        let class = Js::constructor(name, None, move |target, args| {
            let class = class_slot.get().cloned().unwrap_or(Js::Undefined);
            let args: Vec<_> = args.iter().map(to_account).collect();
            body(&*me, to_account(&class), to_account(&target), &args)
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        });
        js(class.define("prototype", prototype))?;
        let _ = slot.set(class.clone());
        Ok(to_account(&class))
    }
    fn arrow(&self, owner: &AccountValue, body: PublicMethod) -> SigningResult<AccountValue> {
        let me = js(self.me())?;
        let owner = owner.clone();
        Ok(to_account(&Js::function(
            body.metadata().name,
            move |_, args| {
                let args: Vec<_> = args.iter().map(to_account).collect();
                body.invoke(&*me, owner.clone(), &args)
                    .map(|v| to_js(&v))
                    .map_err(to_thrown)
            },
        )))
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        js(to_js(object).define(key, to_js(&value)))
    }
    fn get_property(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        js(to_js(object).get(key)).map(|v| to_account(&v))
    }
    fn object_literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::object(
            fields.iter().map(|(k, v)| (*k, to_js(v))).collect(),
        )))
    }
    fn array_literal(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::array(values.iter().map(to_js).collect())))
    }
    fn finish_class_binding(&self, class: &AccountValue) -> SigningResult<()> {
        let class = to_js(class);
        let Js::Function(function) = &class else {
            return Err(to_signing(Thrown::type_error(
                "class binding is not a function",
            )));
        };
        let name = function.name.clone();
        js(self.realm.0.globals.set_key(&name, class))
    }
    fn construct_base(
        &self,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let _ = class;
        let base = self.base.borrow().clone();
        if !base.is_callable() {
            return Err(to_signing(Thrown::type_error(&format!(
                "Class extends value {} is not a constructor",
                base.to_display()
            ))));
        }
        let instance = js(base.call(Js::Undefined, args.iter().map(to_js).collect()))?;
        let prototype = js(to_js(&new_target).get("prototype"))?;
        if let Js::Object(o) = &instance {
            *o.proto.borrow_mut() = Some(prototype);
        }
        Ok(to_account(&instance))
    }
}

/// The `IngressOps<Js, Thrown>` the shared ingress builder makes, seen through
/// the AccountValue boundary the class contexts hand the ingress bodies.
fn account_ops(ops: IngressOps<Js, Thrown>) -> AccountOps {
    let IngressOps {
        undefined,
        boolean,
        number,
        literal,
        object,
        array,
        get,
        index,
        key,
        set,
        set_index,
        delete,
        global,
        invoke,
        function,
        truthy,
        nullish,
        equal,
        text,
        utf16,
        error_value,
        throw,
        spread,
        tuple_two,
        construct,
        is_function,
        is_object,
        for_of,
        less,
        greater,
        less_equal,
        for_of_until,
        subtract,
        multiply,
        add_primitive,
        tdz_error,
    } = ops;
    let a = |v: Js| to_account(&v);
    let all = |vs: Vec<AccountValue>| vs.iter().map(to_js).collect::<Vec<_>>();
    IngressOps {
        undefined: Rc::new(move || a(undefined())),
        boolean: Rc::new(move |b| a(boolean(b))),
        number: Rc::new(move |n| a(number(n))),
        literal: Rc::new(move |s| a(literal(s))),
        object: Rc::new(move |fields| {
            a(object(fields.iter().map(|(k, v)| (*k, to_js(v))).collect()))
        }),
        array: Rc::new(move |items| a(array(all(items)))),
        get: Rc::new(move |o, k| js(get(&to_js(o), k)).map(a)),
        index: Rc::new(move |o, k| js(index(&to_js(o), &to_js(k))).map(a)),
        key: Rc::new(move |k| js(key(&to_js(k))).map(a)),
        set: Rc::new(move |o, k, v| js(set(&to_js(o), k, to_js(&v)))),
        set_index: Rc::new(move |o, k, v| js(set_index(&to_js(o), &to_js(k), to_js(&v)))),
        delete: Rc::new(move |o, k| js(delete(&to_js(o), &to_js(k)))),
        global: Rc::new(move |name| js(global(name)).map(a)),
        invoke: Rc::new(move |c, t, args| js(invoke(to_js(&c), to_js(&t), all(args))).map(a)),
        function: Rc::new(move |name, length, callback| {
            a(function(
                name,
                length,
                Rc::new(move |args: Vec<Js>| {
                    callback(args.iter().map(to_account).collect())
                        .map(|v| to_js(&v))
                        .map_err(to_thrown)
                }),
            ))
        }),
        truthy: Rc::new(move |v| truthy(&to_js(v))),
        nullish: Rc::new(move |v| nullish(&to_js(v))),
        equal: Rc::new(move |x, y| equal(&to_js(x), &to_js(y))),
        text: Rc::new(move |v| js(text(&to_js(v)))),
        utf16: Rc::new(move |units| a(utf16(units))),
        error_value: Rc::new(move |e| a(error_value(to_thrown(e)))),
        throw: Rc::new(move |v| to_signing(throw(to_js(&v)))),
        spread: Rc::new(move |v| js(spread(&to_js(v))).map(|vs| vs.into_iter().map(a).collect())),
        tuple_two: Rc::new(move |v| js(tuple_two(&to_js(v))).map(|(x, y)| (a(x), a(y)))),
        construct: Rc::new(move |c, args| js(construct(to_js(&c), all(args))).map(a)),
        is_function: Rc::new(move |v| is_function(&to_js(v))),
        is_object: Rc::new(move |v| is_object(&to_js(v))),
        for_of: Rc::new(move |v, visit| {
            js(for_of(
                to_js(&v),
                Rc::new(move |item: Js| visit(to_account(&item)).map_err(to_thrown)),
            ))
        }),
        less: Rc::new(move |x, y| js(less(&to_js(x), &to_js(y)))),
        greater: Rc::new(move |x, y| js(greater(&to_js(x), &to_js(y)))),
        less_equal: Rc::new(move |x, y| js(less_equal(&to_js(x), &to_js(y)))),
        for_of_until: Rc::new(move |v, visit| {
            js(for_of_until(
                to_js(&v),
                Rc::new(move |item: Js| visit(to_account(&item)).map_err(to_thrown)),
            ))
        }),
        subtract: Rc::new(move |x, y| js(subtract(to_js(&x), to_js(&y))).map(a)),
        multiply: Rc::new(move |x, y| js(multiply(to_js(&x), to_js(&y))).map(a)),
        add_primitive: Rc::new(move |x, y| js(add_primitive(to_js(&x), to_js(&y))).map(a)),
        tdz_error: Rc::new(move |name| to_signing(tdz_error(name))),
    }
}
