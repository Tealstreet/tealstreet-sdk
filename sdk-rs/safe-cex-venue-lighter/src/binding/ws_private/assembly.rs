//! Language operations `LighterPrivateWebsocket::define` assembles the class with,
//! and the member-family contexts its arrow fields dispatch through.
use super::{
    super::{
        realm::Realm,
        value::{native_to_js, to_account, to_js, to_signing, to_thrown},
    },
    owner::{Owner, Params},
    slots::{self, Feed, Stream},
};
use crate::{
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::{SigningResult, TransactionSigningError},
    ws_private_class::{
        PrivateSocketHost, PrivateWebsocketConstructor, PrivateWebsocketContexts,
        PrivateWebsocketMethod, PrivateWebsocketRealm,
    },
    ws_private_feed_helpers::LighterPrivateFeedRuntime,
    ws_private_stream_handlers::LighterPrivateStreamRuntime,
    ws_private_warnings::{
        PrivateStreamWarningParams, PrivateWarningFlags, PrivateWarningHost, ReadonlyWarningTimes,
    },
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::{cell::OnceCell, rc::Rc};

struct Parts {
    realm: Realm,
    /// The class-wide readonlyFallbackWarningAtByAccount map and its JS view.
    times: ReadonlyWarningTimes,
    times_view: Js,
    feed: OnceCell<Rc<Feed>>,
    stream: OnceCell<Rc<Stream>>,
}

#[derive(Clone)]
pub(super) struct Assembly(Rc<Parts>);

fn js<T>(result: Result<T, Thrown>) -> SigningResult<T> {
    result.map_err(to_signing)
}

impl Assembly {
    pub(super) fn new(realm: Realm) -> Self {
        Self(Rc::new(Parts {
            realm,
            times: ReadonlyWarningTimes::default(),
            times_view: Js::new_map(),
            feed: OnceCell::new(),
            stream: OnceCell::new(),
        }))
    }
    fn owner(&self, owner: AccountValue) -> Rc<Owner> {
        Rc::new(Owner {
            realm: self.0.realm.clone(),
            this: to_js(&owner),
        })
    }
    /// Mirror the native map into its JS view; the warning body is the only writer.
    fn sync_times_view(&self) {
        let Js::Map(view) = &self.0.times_view else {
            return;
        };
        let mut view = view.borrow_mut();
        for (key, at) in self.0.times.borrow().iter() {
            let key = Js::utf16(key);
            match view.iter_mut().find(|(k, _)| k.strict_equals(&key)) {
                Some((_, value)) => *value = Js::Number(*at),
                None => view.push((key, Js::Number(*at))),
            }
        }
    }
}

impl PrivateWebsocketContexts<Realm> for Assembly {
    fn socket(&self, owner: AccountValue) -> Rc<dyn PrivateSocketHost> {
        self.owner(owner)
    }
    fn warnings(&self, owner: AccountValue) -> Rc<dyn PrivateWarningHost> {
        self.owner(owner)
    }
    fn warning_flags(&self, owner: AccountValue) -> Rc<dyn PrivateWarningFlags> {
        self.owner(owner)
    }
    fn warning_params(&self, params: AccountValue) -> Rc<dyn PrivateStreamWarningParams> {
        Rc::new(Params(to_js(&params)))
    }
    fn readonly_warning_times(&self) -> ReadonlyWarningTimes {
        self.0.times.clone()
    }
    fn feed(&self) -> Rc<LighterPrivateFeedRuntime<Realm, AccountValue>> {
        self.0
            .feed
            .get_or_init(|| Rc::new(slots::feed(&self.0.realm)))
            .clone()
    }
    fn stream(&self) -> Rc<LighterPrivateStreamRuntime<Realm, AccountValue>> {
        self.0
            .stream
            .get_or_init(|| Rc::new(slots::stream(self.feed())))
            .clone()
    }
    fn flatten_by_market_payload(
        &self,
        payload: &AccountValue,
        include_single_objects: bool,
    ) -> SigningResult<AccountValue> {
        js(crate::ws_private::flatten_by_market_payload(
            Some(&to_js(payload)),
            include_single_objects,
        ))
        .map(|value| to_account(&value))
    }
    fn empty_array(&self) -> AccountValue {
        to_account(&Js::array(vec![]))
    }
    fn type_error(&self, message: &str) -> TransactionSigningError {
        to_signing(Thrown::type_error(message))
    }
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.0.realm.exchange_runtime(), value))
    }
}

impl PrivateWebsocketRealm<Realm> for Assembly {
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue> {
        js(self.0.realm.global(name)).map(|v| to_account(&v))
    }
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        body: PrivateWebsocketConstructor<Realm>,
    ) -> SigningResult<AccountValue> {
        let _ = length;
        let parent = js(to_js(&base).get("prototype"))?;
        let prototype = Js::object(vec![]);
        if let Js::Object(o) = &prototype {
            *o.proto.borrow_mut() = (!parent.is_nullish()).then_some(parent);
        }
        let assembly = self.clone();
        let slot: Rc<OnceCell<Js>> = Rc::default();
        let class_slot = slot.clone();
        let class = Js::constructor(name, None, move |target, args| {
            let class = class_slot.get().cloned().unwrap_or(Js::Undefined);
            let class = to_account(&class);
            let args: Vec<_> = args.iter().map(to_account).collect();
            body(&assembly, class, to_account(&target), &args)
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        });
        js(class.define("prototype", prototype))?;
        js(class.set("__proto__", to_js(&base)))?;
        js(class.define("name", Js::str(name)))?;
        let _ = slot.set(class.clone());
        Ok(to_account(&class))
    }
    fn arrow(
        &self,
        owner: &AccountValue,
        body: PrivateWebsocketMethod,
    ) -> SigningResult<AccountValue> {
        let assembly = self.clone();
        let owner = owner.clone();
        let function = Js::function(body.metadata().name, move |_, args| {
            let args: Vec<_> = args.iter().map(to_account).collect();
            let result = body.invoke(&assembly, owner.clone(), &args);
            if body == PrivateWebsocketMethod::EmitReadonlyFallbackWarning {
                assembly.sync_times_view();
            }
            result.map(|v| to_js(&v)).map_err(to_thrown)
        });
        Ok(to_account(&function))
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        js(to_js(object).define(key, to_js(&value)))
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        js(to_js(object).get(key)).map(|v| to_account(&v))
    }
    fn readonly_warning_map(&self) -> SigningResult<AccountValue> {
        Ok(to_account(&self.0.times_view))
    }
    fn finish_class_binding(&self, class: &AccountValue) -> SigningResult<()> {
        let class = to_js(class);
        let name = js(class.get("name").and_then(|n| n.to_js_string()))?;
        js(self.0.realm.0.globals.set_key(&name, class))
    }
    fn construct_base(
        &self,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let base = js(to_js(&class).super_constructor())?;
        let instance =
            js(base.construct(args.iter().map(to_js).collect(), Some(&to_js(&new_target))))?;
        let prototype = js(to_js(&new_target).get("prototype"))?;
        if let Js::Object(o) = &instance {
            *o.proto.borrow_mut() = Some(prototype);
        }
        Ok(to_account(&instance))
    }
}
