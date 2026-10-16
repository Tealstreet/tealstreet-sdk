//! lighter.signer.ts: the TealstreetLighterSignerAdapter class and the
//! createLighterSignerAdapter factory, assembled by the ported
//! `signer_adapter`/`signer_runtime` bodies over js_runtime values.
use super::{Host, NativeSlots, error, has_prototype, instance_of};
use crate::{
    binding::{
        ops::{Arithmetic, arithmetic, to_string},
        value::{to_signing, to_thrown},
    },
    signer_adapter::{
        SignerAdapterConstructor, SignerAdapterMethod, SignerAdapterRealm,
        create_lighter_signer_adapter,
    },
    signer_helpers::is_lighter_invalid_nonce_error,
    signer_runtime::{LighterSignerRuntime, TealstreetLighterSignerAdapter},
    transaction_promise::NativePromise,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg, to_json};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};

type Adapter = TealstreetLighterSignerAdapter<Host, Js>;
type Runtime = LighterSignerRuntime<Host, Js>;

fn global_call(h: &Host, name: &str, args: Vec<Js>) -> SigningResult<Js> {
    h.global(name)
        .and_then(|f| f.call(Js::Undefined, args))
        .map_err(to_signing)
}

/// The imported capabilities lighter.signer.ts resolves by module binding.
fn runtime(host: &Host) -> Runtime {
    LighterSignerRuntime {
        promise_runtime: host.microtasks.clone(),
        host: host.clone(),
        get: |_, value, key| value.get(key).map_err(to_signing),
        method: |_, receiver, name| {
            let member = receiver.get(name).map_err(to_signing)?;
            if !member.is_callable() {
                return Err(to_signing(Thrown::type_error(&format!(
                    "{name} is not a function"
                ))));
            }
            Ok(Js::function(name, move |_, a| {
                member.call(receiver.clone(), a)
            }))
        },
        call: |_, callee, args| callee.call(Js::Undefined, args).map_err(to_signing),
        call_async: |h, callee, args| {
            let value = callee.call(Js::Undefined, args).map_err(to_signing)?;
            Ok(h.awaited(value, Ok))
        },
        // ts: new TransactionClient(config)
        construct_transaction_client: |h, config| global_call(h, "TransactionClient", vec![config]),
        object: Js::object,
        array: Js::array,
        undefined: || Js::Undefined,
        text: Js::str,
        truthy: Js::truthy,
        nullish: Js::is_nullish,
        strict_zero: |value| value.strict_equals(&Js::Number(0.0)),
        wasm_ready: |h| global_call(h, "isWasmReady", vec![]).map(|ready| ready.truthy()),
        init_wasm: |h| global_call(h, "initWasm", vec![]).map(|init| h.awaited(init, Ok)),
        crypto_api: |h| global_call(h, "getCryptoApi", vec![]),
        deployment_profile: |h, input| {
            global_call(h, "resolveLighterDeploymentProfile", vec![input])
        },
        error_value: |e| to_thrown(e.clone()).value,
        lowercase_string: |_, value| {
            let text = to_string(&value).map_err(to_signing)?;
            Ok(String::from_utf16_lossy(&text)
                .to_lowercase()
                .encode_utf16()
                .collect())
        },
        // ts: isLighterInvalidNonceError, over the thrown value's JSON view.
        invalid_nonce: |_, e| {
            let value = to_json(&to_thrown(e.clone()).value).map_err(to_signing)?;
            is_lighter_invalid_nonce_error(Some(&value)).map_err(|c| to_signing(c.into()))
        },
        // ts: new Promise((resolve) => setTimeout(resolve, ms))
        delay: |h, ms| {
            let (promise, resolver) = NativePromise::pending(h.microtasks.clone());
            let resolver = RefCell::new(Some(resolver));
            let resolve = Js::function("resolve", move |_, _| {
                if let Some(resolver) = resolver.borrow_mut().take() {
                    resolver.settle(Ok(Js::Undefined));
                }
                Ok(Js::Undefined)
            });
            global_call(h, "setTimeout", vec![resolve, Js::Number(f64::from(ms))])?;
            Ok(promise)
        },
        new_error: |_, message| to_signing(Thrown::error("Error", message)),
        number: Js::Number,
        add: |_, left, right| arithmetic(Arithmetic::Add, &left, &right).map_err(to_signing),
        big_int: |h, value| global_call(h, "BigInt", vec![value]),
    }
}

/// The class realm: one per venue realm, holding the module-lexical class.
struct AdapterRealm {
    host: Host,
    runtime: Rc<Runtime>,
    class: OnceCell<Js>,
    natives: NativeSlots<Adapter>,
    this: Weak<AdapterRealm>,
}

fn upgrade(realm: &Weak<AdapterRealm>) -> Result<Rc<AdapterRealm>, Thrown> {
    realm
        .upgrade()
        .ok_or_else(|| Thrown::error("Error", "Lighter signer realm was dropped"))
}

impl AdapterRealm {
    /// A callable dispatching to `method` on the adapter `resolve` yields.
    fn callable(
        &self,
        method: SignerAdapterMethod,
        resolve: impl Fn(&AdapterRealm, &Js) -> Option<Rc<Adapter>> + 'static,
    ) -> Js {
        let realm = self.this.clone();
        let name = method.metadata().name;
        Js::function(name, move |this, a| {
            let realm = upgrade(&realm)?;
            Ok(match resolve(&realm, &this) {
                Some(adapter) => {
                    let result = method.invoke(realm.as_ref(), &adapter, arg(&a, 0));
                    realm.host.promise(result, |value| Ok(value.clone()))
                }
                None => realm.host.rejected(Thrown::type_error(&format!(
                    "TealstreetLighterSignerAdapter.{name} called on an incompatible receiver"
                ))),
            })
        })
    }
}

impl SignerAdapterRealm<Host, Js> for AdapterRealm {
    fn runtime(&self) -> Rc<Runtime> {
        self.runtime.clone()
    }
    fn create_class(
        &self,
        name: &'static str,
        length: usize,
        body: SignerAdapterConstructor<Host, Js>,
    ) -> SigningResult<Js> {
        let _ = length;
        let prototype = Js::object(vec![]);
        let (realm, own_prototype) = (self.this.clone(), prototype.clone());
        let class = Js::constructor(name, move |this, a| {
            if !has_prototype(&this, &own_prototype) {
                return Err(Thrown::type_error(&format!(
                    "Class constructor {name} cannot be invoked without 'new'"
                )));
            }
            let realm = upgrade(&realm)?;
            body(realm.as_ref(), this, &a).map_err(to_thrown)
        });
        class.define("prototype", prototype).map_err(to_signing)?;
        Ok(class)
    }
    fn define_prototype_method(
        &self,
        class: &Js,
        method: SignerAdapterMethod,
    ) -> SigningResult<()> {
        let prototype = class.get("prototype").map_err(to_signing)?;
        let callable = self.callable(method, |realm, this| realm.natives.get(this));
        prototype
            .define(method.metadata().name, callable)
            .map_err(to_signing)
    }
    fn bind_native(&self, receiver: &Js, adapter: Rc<Adapter>) -> SigningResult<()> {
        self.natives.bind(receiver, adapter).map_err(to_signing)
    }
    fn define_field_fn(
        &self,
        receiver: &Js,
        method: SignerAdapterMethod,
        adapter: Rc<Adapter>,
    ) -> SigningResult<()> {
        let bound = Rc::downgrade(&adapter);
        drop(adapter);
        // Lexical `this`: the receiver's own adapter, whatever the call receiver.
        let callable = self.callable(method, move |_, _| bound.upgrade());
        receiver
            .define(method.metadata().name, callable)
            .map_err(to_signing)
    }
    fn finish_class_binding(&self, class: &Js) -> SigningResult<()> {
        self.class
            .set(class.clone())
            .map_err(|_| to_signing(Thrown::error("Error", "class bound twice")))
    }
    fn class_binding(&self) -> SigningResult<Js> {
        self.class.get().cloned().ok_or_else(|| {
            to_signing(Thrown::error(
                "ReferenceError",
                "Cannot access 'TealstreetLighterSignerAdapter' before initialization",
            ))
        })
    }
    fn construct(&self, class: Js, args: Vec<Js>) -> SigningResult<Js> {
        let prototype = class.get("prototype").map_err(to_signing)?;
        let receiver = instance_of(&prototype);
        let result = class.call(receiver.clone(), args).map_err(to_signing)?;
        Ok(if result.is_object() { result } else { receiver })
    }
    fn strict_equals_text(&self, value: &Js, text: &str) -> bool {
        value.strict_equals(&Js::str(text))
    }
    fn template_string(&self, value: Js) -> SigningResult<Vec<u16>> {
        to_string(&value).map(|s| s.to_vec()).map_err(to_signing)
    }
    fn new_error(&self, message: Vec<u16>) -> TransactionSigningError {
        to_signing(error(&message))
    }
}

pub(super) fn install(host: &Host) -> Result<(), Thrown> {
    let realm = Rc::new_cyclic(|this| AdapterRealm {
        host: host.clone(),
        runtime: Rc::new(runtime(host)),
        class: OnceCell::new(),
        natives: NativeSlots::default(),
        this: this.clone(),
    });
    TealstreetLighterSignerAdapter::define(realm.as_ref()).map_err(to_thrown)?;
    // ts: createLighterSignerAdapter
    let factory = Js::function("createLighterSignerAdapter", move |_, a| {
        let created = create_lighter_signer_adapter(realm.as_ref(), arg(&a, 0));
        Ok(realm.host.promise(created, |adapter| Ok(adapter.clone())))
    });
    host.define_global("createLighterSignerAdapter", factory)
}
