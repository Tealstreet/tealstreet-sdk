//! Signer adapter interface, class assembly and provider-checked factory.
use crate::{
    exchange_class::MethodMetadata,
    signer_runtime::{LighterSignerRuntime, TealstreetLighterSignerAdapter},
    transaction_promise::NativePromise,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;

// ts: LighterSignerAdapter
pub trait LighterSignerAdapter<V> {
    fn initialize(&self) -> NativePromise<V>;
    fn create_order(&self, request: V) -> NativePromise<V>;
    fn create_orders(&self, requests: V) -> NativePromise<V>;
    fn create_grouped_orders(&self, request: V) -> NativePromise<V>;
    fn cancel_order(&self, request: V) -> NativePromise<V>;
    fn update_leverage(&self, request: V) -> NativePromise<V>;
    fn approve_integrator(&self, request: V) -> NativePromise<V>;
    fn get_auth_token(&self, ttl_seconds: V) -> NativePromise<V>;
    /// Optional member: `None` is an absent `dispose` property.
    fn dispose(&self) -> Option<NativePromise<V>> {
        None
    }
}

impl<H: 'static, V: Clone + 'static> LighterSignerAdapter<V>
    for Rc<TealstreetLighterSignerAdapter<H, V>>
{
    fn initialize(&self) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::initialize(self)
    }
    fn create_order(&self, request: V) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::create_order(self, request)
    }
    fn create_orders(&self, requests: V) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::create_orders(self, requests)
    }
    fn create_grouped_orders(&self, request: V) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::create_grouped_orders(self, request)
    }
    fn cancel_order(&self, request: V) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::cancel_order(self, request)
    }
    fn update_leverage(&self, request: V) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::update_leverage(self, request)
    }
    fn approve_integrator(&self, request: V) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::approve_integrator(self, request)
    }
    fn get_auth_token(&self, ttl_seconds: V) -> NativePromise<V> {
        TealstreetLighterSignerAdapter::get_auth_token(self, ttl_seconds)
    }
}

/// Sealed selector for the class field-fn and every prototype method body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignerAdapterMethod {
    RunWithSubmissionLock,
    Initialize,
    CreateOrder,
    CreateOrders,
    CreateGroupedOrders,
    CancelOrder,
    UpdateLeverage,
    ApproveIntegrator,
    GetAuthToken,
}
/// Prototype methods in source declaration order.
const PROTOTYPE_METHODS: [SignerAdapterMethod; 8] = [
    SignerAdapterMethod::Initialize,
    SignerAdapterMethod::CreateOrder,
    SignerAdapterMethod::CreateOrders,
    SignerAdapterMethod::CreateGroupedOrders,
    SignerAdapterMethod::CancelOrder,
    SignerAdapterMethod::UpdateLeverage,
    SignerAdapterMethod::ApproveIntegrator,
    SignerAdapterMethod::GetAuthToken,
];
impl SignerAdapterMethod {
    pub fn metadata(self) -> MethodMetadata {
        let (name, length) = match self {
            Self::RunWithSubmissionLock => ("runWithSubmissionLock", 1),
            Self::Initialize => ("initialize", 0),
            Self::CreateOrder => ("createOrder", 1),
            Self::CreateOrders => ("createOrders", 1),
            Self::CreateGroupedOrders => ("createGroupedOrders", 1),
            Self::CancelOrder => ("cancelOrder", 1),
            Self::UpdateLeverage => ("updateLeverage", 1),
            Self::ApproveIntegrator => ("approveIntegrator", 1),
            Self::GetAuthToken => ("getAuthToken", 1),
        };
        MethodMetadata {
            name,
            length,
            asynchronous: true,
        }
    }
    /// Direct dispatch from a class callable to the receiver's native body;
    /// `argument` is the first call argument, or undefined when omitted.
    pub fn invoke<H: 'static, V: Clone + 'static>(
        self,
        realm: &dyn SignerAdapterRealm<H, V>,
        adapter: &Rc<TealstreetLighterSignerAdapter<H, V>>,
        argument: V,
    ) -> NativePromise<V> {
        match self {
            Self::RunWithSubmissionLock => {
                let runtime = realm.runtime();
                adapter.run_with_submission_lock(move || {
                    (runtime.call_async)(&runtime.host, argument, vec![])
                })
            }
            Self::Initialize => adapter.initialize(),
            Self::CreateOrder => adapter.create_order(argument),
            Self::CreateOrders => adapter.create_orders(argument),
            Self::CreateGroupedOrders => adapter.create_grouped_orders(argument),
            Self::CancelOrder => adapter.cancel_order(argument),
            Self::UpdateLeverage => adapter.update_leverage(argument),
            Self::ApproveIntegrator => adapter.approve_integrator(argument),
            Self::GetAuthToken => adapter.get_auth_token(argument),
        }
    }
}

pub type SignerAdapterConstructor<H, V> =
    fn(&dyn SignerAdapterRealm<H, V>, V, &[V]) -> SigningResult<V>;

/// Required language operations for the class and its factory.
pub trait SignerAdapterRealm<H, V> {
    /// Imported capabilities shared by every instance of this class realm.
    fn runtime(&self) -> Rc<LighterSignerRuntime<H, V>>;
    /// Base class whose [[Construct]] allocates the receiver, then runs `body`.
    fn create_class(
        &self,
        name: &'static str,
        length: usize,
        body: SignerAdapterConstructor<H, V>,
    ) -> SigningResult<V>;
    /// Non-enumerable writable configurable prototype method, dynamic this.
    fn define_prototype_method(&self, class: &V, method: SignerAdapterMethod) -> SigningResult<()>;
    /// Associate the receiver with the native state its methods dispatch to.
    fn bind_native(
        &self,
        receiver: &V,
        adapter: Rc<TealstreetLighterSignerAdapter<H, V>>,
    ) -> SigningResult<()>;
    /// DefineField own arrow whose lexical this is the bound native adapter.
    fn define_field_fn(
        &self,
        receiver: &V,
        method: SignerAdapterMethod,
        adapter: Rc<TealstreetLighterSignerAdapter<H, V>>,
    ) -> SigningResult<()>;
    fn finish_class_binding(&self, class: &V) -> SigningResult<()>;
    /// Module-lexical `TealstreetLighterSignerAdapter` binding.
    fn class_binding(&self) -> SigningResult<V>;
    /// `new class(...args)`.
    fn construct(&self, class: V, args: Vec<V>) -> SigningResult<V>;
    /// `value === text` for a string literal operand.
    fn strict_equals_text(&self, value: &V, text: &str) -> bool;
    /// Template-literal substitution ToString, including its Symbol TypeError.
    fn template_string(&self, value: V) -> SigningResult<Vec<u16>>;
    /// `new Error(message)`.
    fn new_error(&self, message: Vec<u16>) -> TransactionSigningError;
}

impl<H: 'static, V: Clone + 'static> TealstreetLighterSignerAdapter<H, V> {
    pub fn define(realm: &dyn SignerAdapterRealm<H, V>) -> SigningResult<V> {
        let class = realm.create_class("TealstreetLighterSignerAdapter", 5, Self::construct)?;
        for method in PROTOTYPE_METHODS {
            realm.define_prototype_method(&class, method)?;
        }
        realm.finish_class_binding(&class)?;
        Ok(class)
    }
    pub fn construct(
        realm: &dyn SignerAdapterRealm<H, V>,
        receiver: V,
        args: &[V],
    ) -> SigningResult<V> {
        let runtime = realm.runtime();
        let arg = |index: usize| {
            args.get(index)
                .cloned()
                .unwrap_or_else(|| (runtime.undefined)())
        };
        let adapter = Self::new(runtime.clone(), arg(0), arg(1), arg(2), arg(3), arg(4));
        realm.bind_native(&receiver, adapter.clone())?;
        realm.define_field_fn(
            &receiver,
            SignerAdapterMethod::RunWithSubmissionLock,
            adapter,
        )?;
        Ok(receiver)
    }
}

const UNSUPPORTED_PREFIX: &str = "Unsupported Lighter signer provider \"";
const UNSUPPORTED_SUFFIX: &str =
    "\". Legacy third-party signer providers are removed; use tealstreet-sdk.";

// ts: createLighterSignerAdapter
pub fn create_lighter_signer_adapter<H: 'static, V: Clone + 'static>(
    realm: &dyn SignerAdapterRealm<H, V>,
    params: V,
) -> NativePromise<V> {
    let runtime = realm.runtime();
    let r = &runtime;
    let h = &r.host;
    let (promise, out) = NativePromise::pending(r.promise_runtime.clone());
    let started = (|| {
        // getProvider: options.extra?.lighterSignerProvider over live host values.
        let options = (r.get)(h, &params, "options")?;
        let extra = (r.get)(h, &options, "extra")?;
        let provider = if (r.nullish)(&extra) {
            (r.undefined)()
        } else {
            (r.get)(h, &extra, "lighterSignerProvider")?
        };
        if (r.truthy)(&provider) && !realm.strict_equals_text(&provider, "tealstreet-sdk") {
            let mut message: Vec<u16> = UNSUPPORTED_PREFIX.encode_utf16().collect();
            message.extend(realm.template_string(provider)?);
            message.extend(UNSUPPORTED_SUFFIX.encode_utf16());
            return Err(realm.new_error(message));
        }
        let class = realm.class_binding()?;
        let mut args = vec![];
        for key in [
            "options",
            "exchangeName",
            "apiClient",
            "accountIndex",
            "apiKeyIndex",
        ] {
            args.push((r.get)(h, &params, key)?);
        }
        let adapter = realm.construct(class, args)?;
        let initialize = (r.method)(h, adapter.clone(), "initialize")?;
        Ok(((r.call_async)(h, initialize, vec![])?, adapter))
    })();
    match started {
        Err(error) => out.settle(Err(error)),
        Ok((initialized, adapter)) => initialized.subscribe(move |value| match value {
            Err(error) => out.settle(Err(error)),
            Ok(_) => out.settle(Ok(adapter)),
        }),
    }
    promise
}
