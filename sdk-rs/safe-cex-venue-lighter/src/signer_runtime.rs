//! Signer initialization and serialized submission over imported SDK capabilities.
use crate::{
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::{cell::RefCell, rc::Rc};

pub struct LighterSignerRuntime<H, V> {
    pub promise_runtime: Rc<dyn TransactionRuntime>,
    pub host: H,
    pub get: fn(&H, &V, &str) -> SigningResult<V>,
    pub method: fn(&H, V, &'static str) -> SigningResult<V>,
    pub call: fn(&H, V, Vec<V>) -> SigningResult<V>,
    pub call_async: fn(&H, V, Vec<V>) -> SigningResult<NativePromise<V>>,
    pub construct_transaction_client: fn(&H, V) -> SigningResult<V>,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub undefined: fn() -> V,
    pub text: fn(&str) -> V,
    pub truthy: fn(&V) -> bool,
    pub nullish: fn(&V) -> bool,
    pub strict_zero: fn(&V) -> bool,
    pub wasm_ready: fn(&H) -> SigningResult<bool>,
    pub init_wasm: fn(&H) -> SigningResult<NativePromise<V>>,
    pub crypto_api: fn(&H) -> SigningResult<V>,
    pub deployment_profile: fn(&H, V) -> SigningResult<V>,
    pub error_value: fn(&TransactionSigningError) -> V,
    pub lowercase_string: fn(&H, V) -> SigningResult<Vec<u16>>,
    pub invalid_nonce: fn(&H, &TransactionSigningError) -> SigningResult<bool>,
    pub delay: fn(&H, u32) -> SigningResult<NativePromise<V>>,
    pub new_error: fn(&H, &'static str) -> TransactionSigningError,
    pub number: fn(f64) -> V,
    /// JavaScript binary `+`, including ToPrimitive and string concatenation.
    pub add: fn(&H, V, V) -> SigningResult<V>,
    /// Global `BigInt(value)` call, including its RangeError/SyntaxError/TypeError throws.
    pub big_int: fn(&H, V) -> SigningResult<V>,
}

pub struct TealstreetLighterSignerAdapter<H, V> {
    pub options: V,
    pub exchange_name: V,
    pub api_client: V,
    pub account_index: V,
    pub api_key_index: V,
    pub transaction_client: RefCell<V>,
    submission_lock: RefCell<NativePromise<()>>,
    runtime: Rc<LighterSignerRuntime<H, V>>,
}

impl<H: 'static, V: Clone + 'static> TealstreetLighterSignerAdapter<H, V> {
    pub fn new(
        runtime: Rc<LighterSignerRuntime<H, V>>,
        options: V,
        exchange_name: V,
        api_client: V,
        account_index: V,
        api_key_index: V,
    ) -> Rc<Self> {
        let submission_lock = NativePromise::settled(runtime.promise_runtime.clone(), Ok(()));
        Rc::new(Self {
            options,
            exchange_name,
            api_client,
            account_index,
            api_key_index,
            transaction_client: RefCell::new((runtime.undefined)()),
            submission_lock: RefCell::new(submission_lock),
            runtime,
        })
    }

    pub fn run_with_submission_lock(
        self: &Rc<Self>,
        callback: impl FnOnce() -> SigningResult<NativePromise<V>> + 'static,
    ) -> NativePromise<V> {
        let r = &self.runtime;
        let (out, result) = NativePromise::pending(r.promise_runtime.clone());
        let (next, release) = NativePromise::pending(r.promise_runtime.clone());
        let prev = self.submission_lock.replace(next);
        prev.subscribe(move |previous| match previous {
            Err(error) => result.settle(Err(error)),
            Ok(_) => match callback() {
                Err(error) => {
                    release.settle(Ok(()));
                    result.settle(Err(error));
                }
                Ok(promise) => promise.subscribe(move |value| {
                    release.settle(Ok(()));
                    result.settle_shared(value);
                }),
            },
        });
        out
    }

    fn has_client(&self) -> bool {
        let client = self.transaction_client.borrow().clone();
        (self.runtime.truthy)(&client)
    }

    fn build_client(&self) -> SigningResult<V> {
        let r = &self.runtime;
        let h = &r.host;
        let crypto = (r.crypto_api)(h)?;
        let profile = (r.deployment_profile)(
            h,
            (r.object)(vec![
                ("exchangeName", self.exchange_name.clone()),
                ("testnet", (r.get)(h, &self.options, "testnet")?),
            ]),
        )?;
        let client = (r.construct_transaction_client)(
            h,
            (r.object)(vec![
                ("httpClient", self.api_client.clone()),
                ("crypto", crypto),
                ("chainId", (r.get)(h, &profile, "signerChainId")?),
                ("privateKey", (r.get)(h, &self.options, "secret")?),
                ("accountIndex", self.account_index.clone()),
                ("apiKeyIndex", self.api_key_index.clone()),
            ]),
        )?;
        self.transaction_client.replace(client);
        Ok((r.undefined)())
    }

    fn initialization_error(
        self: Rc<Self>,
        error: TransactionSigningError,
        out: PromiseResolver<V>,
    ) {
        let r = &self.runtime;
        let h = &r.host;
        let matching = (|| {
            let original = (r.error_value)(&error);
            let message = if (r.nullish)(&original) {
                (r.undefined)()
            } else {
                (r.get)(h, &original, "message")?
            };
            let message = if (r.truthy)(&message) {
                message
            } else if (r.truthy)(&original) {
                original
            } else {
                (r.text)("")
            };
            let message = (r.lowercase_string)(h, message)?;
            let needle: Vec<u16> = "wasm not initialized".encode_utf16().collect();
            Ok(message.windows(needle.len()).any(|part| part == needle))
        })();
        match matching {
            Err(error) => out.settle(Err(error)),
            Ok(false) => out.settle(Err(error)),
            Ok(true) => match (r.init_wasm)(h) {
                Err(error) => out.settle(Err(error)),
                Ok(promise) => promise.subscribe(move |value| match value {
                    Err(error) => out.settle(Err(error)),
                    Ok(_) => out.settle(self.build_client()),
                }),
            },
        }
    }

    pub fn initialize(self: &Rc<Self>) -> NativePromise<V> {
        let r = &self.runtime;
        let h = &r.host;
        let (promise, out) = NativePromise::pending(r.promise_runtime.clone());
        if self.has_client() {
            out.settle(Ok((r.undefined)()));
            return promise;
        }
        match (r.get)(h, &self.options, "secret") {
            Err(error) => out.settle(Err(error)),
            Ok(secret) if !(r.truthy)(&secret) => {
                out.settle(Err((r.new_error)(
                    h,
                    "Missing secret for signer initialization",
                )));
            }
            Ok(_) => match (r.wasm_ready)(h) {
                Err(error) => self.clone().initialization_error(error, out),
                Ok(true) => match self.build_client() {
                    Ok(value) => out.settle(Ok(value)),
                    Err(error) => self.clone().initialization_error(error, out),
                },
                Ok(false) => match (r.init_wasm)(h) {
                    Err(error) => self.clone().initialization_error(error, out),
                    Ok(init) => {
                        let this = self.clone();
                        init.subscribe(move |value| match value {
                            Err(error) => this.initialization_error(error, out),
                            Ok(_) => match this.build_client() {
                                Ok(value) => out.settle(Ok(value)),
                                Err(error) => this.initialization_error(error, out),
                            },
                        });
                    }
                },
            },
        }
        promise
    }

    fn reset_nonce(&self) -> SigningResult<V> {
        let r = &self.runtime;
        let client = self.transaction_client.borrow().clone();
        let method = (r.method)(&r.host, client, "resetNonce")?;
        (r.call)(&r.host, method, vec![])
    }

    fn submit_once(self: Rc<Self>, method: &'static str, request: V) -> NativePromise<V> {
        let r = &self.runtime;
        let (promise, out) = NativePromise::pending(r.promise_runtime.clone());
        let signed = (|| {
            let client = self.transaction_client.borrow().clone();
            let callable = (r.method)(&r.host, client, method)?;
            (r.call_async)(&r.host, callable, vec![request])
        })();
        match signed {
            Err(error) => out.settle(Err(error)),
            Ok(signed) => signed.subscribe(move |value| match value {
                Err(error) => out.settle(Err(error)),
                Ok(tx) => {
                    let r = &self.runtime;
                    let batch = method == "getCreateOrderTransactions";
                    let sent = (|| {
                        let callable = (r.method)(
                            &r.host,
                            self.api_client.clone(),
                            if batch { "sendRawTxBatch" } else { "sendRawTx" },
                        )?;
                        (r.call_async)(&r.host, callable, vec![(*tx).clone()])
                    })();
                    match sent {
                        Err(error) => out.settle(Err(error)),
                        Ok(sent) if !batch => sent.subscribe(move |value| out.settle_shared(value)),
                        Ok(sent) => sent.subscribe(move |value| match value {
                            Err(error) => out.settle(Err(error)),
                            Ok(response) => out.settle((self.runtime.get)(
                                &self.runtime.host,
                                response.as_ref(),
                                "txHashes",
                            )),
                        }),
                    }
                }
            }),
        }
        promise
    }

    fn settle_submission(
        &self,
        out: PromiseResolver<V>,
        value: crate::transaction_promise::PromiseResult<V>,
        void: bool,
    ) {
        if void && value.is_ok() {
            out.settle(Ok((self.runtime.undefined)()));
        } else {
            out.settle_shared(value);
        }
    }

    fn submission_error(
        self: Rc<Self>,
        method: &'static str,
        request: V,
        void: bool,
        error: TransactionSigningError,
        out: PromiseResolver<V>,
    ) {
        let r = &self.runtime;
        let retry = self
            .reset_nonce()
            .and_then(|_| (r.invalid_nonce)(&r.host, &error));
        match retry {
            Err(error) => out.settle(Err(error)),
            Ok(false) => out.settle(Err(error)),
            Ok(true) => match (r.delay)(&r.host, 150) {
                Err(error) => out.settle(Err(error)),
                Ok(delay) => delay.subscribe(move |value| match value {
                    Err(error) => out.settle(Err(error)),
                    Ok(_) => {
                        let retry = self.clone().submit_once(method, request);
                        let settle = move |value| match value {
                            Err(error) => match self.reset_nonce() {
                                Err(error) => out.settle(Err(error)),
                                Ok(_) => out.settle(Err(error)),
                            },
                            Ok(value) => self.settle_submission(out, Ok(value), void),
                        };
                        retry.subscribe(settle);
                    }
                }),
            },
        }
    }

    fn submit_ready(
        self: Rc<Self>,
        method: &'static str,
        request: V,
        void: bool,
        out: PromiseResolver<V>,
    ) {
        let r = &self.runtime;
        let empty = (|| {
            if !self.has_client() {
                return Err((r.new_error)(&r.host, "Signer not initialized"));
            }
            Ok(method == "getCreateOrderTransactions"
                && (r.strict_zero)(&(r.get)(&r.host, &request, "length")?))
        })();
        match empty {
            Err(error) => out.settle(Err(error)),
            Ok(true) => out.settle(Ok((r.array)(vec![]))),
            Ok(false) => {
                let promise = self.clone().submit_once(method, request.clone());
                let this = self.clone();
                promise.subscribe(move |value| match value {
                    Err(error) => this.submission_error(method, request, void, error, out),
                    Ok(value) => this.settle_submission(out, Ok(value), void),
                });
            }
        }
    }

    fn submission_body(
        self: Rc<Self>,
        method: &'static str,
        request: V,
        void: bool,
    ) -> NativePromise<V> {
        let r = &self.runtime;
        let (promise, out) = NativePromise::pending(r.promise_runtime.clone());
        if self.has_client() {
            self.submit_ready(method, request, void, out);
        } else {
            let init = self.initialize();
            init.subscribe(move |value| match value {
                Err(error) => out.settle(Err(error)),
                Ok(_) => self.submit_ready(method, request, void, out),
            });
        }
        promise
    }

    fn submit(self: &Rc<Self>, method: &'static str, request: V, void: bool) -> NativePromise<V> {
        let (promise, out) = NativePromise::pending(self.runtime.promise_runtime.clone());
        let this = self.clone();
        self.run_with_submission_lock(move || Ok(this.submission_body(method, request, void)))
            .subscribe(move |value| out.settle_shared(value));
        promise
    }

    pub fn create_order(self: &Rc<Self>, request: V) -> NativePromise<V> {
        self.submit("getCreateOrderTransaction", request, false)
    }
    pub fn create_orders(self: &Rc<Self>, requests: V) -> NativePromise<V> {
        self.submit("getCreateOrderTransactions", requests, false)
    }
    pub fn create_grouped_orders(self: &Rc<Self>, request: V) -> NativePromise<V> {
        self.submit("getCreateGroupedOrdersTransaction", request, false)
    }
    pub fn cancel_order(self: &Rc<Self>, request: V) -> NativePromise<V> {
        self.submit("getCancelOrderTransaction", request, false)
    }
    pub fn update_leverage(self: &Rc<Self>, request: V) -> NativePromise<V> {
        self.submit("getUpdateLeverageTransaction", request, true)
    }
    pub fn approve_integrator(self: &Rc<Self>, request: V) -> NativePromise<V> {
        self.submit("getApproveIntegratorTransaction", request, false)
    }

    // ts: TealstreetLighterSignerAdapter.getAuthToken
    pub fn get_auth_token(self: &Rc<Self>, ttl_seconds: V) -> NativePromise<V> {
        let (promise, out) = NativePromise::pending(self.runtime.promise_runtime.clone());
        if self.has_client() {
            self.clone().auth_token_ready(ttl_seconds, out);
        } else {
            let this = self.clone();
            self.initialize().subscribe(move |value| match value {
                Err(error) => out.settle(Err(error)),
                Ok(_) => this.auth_token_ready(ttl_seconds, out),
            });
        }
        promise
    }

    fn auth_token_ready(self: Rc<Self>, ttl_seconds: V, out: PromiseResolver<V>) {
        let r = &self.runtime;
        let h = &r.host;
        let token = (|| {
            if !self.has_client() {
                return Err((r.new_error)(h, "Signer not initialized"));
            }
            let seconds = (r.number)((r.promise_runtime.now_millis() / 1000.0).floor());
            let deadline = (r.big_int)(h, (r.add)(h, seconds, ttl_seconds)?)?;
            let client = self.transaction_client.borrow().clone();
            let callable = (r.method)(h, client, "getAuthToken")?;
            (r.call_async)(h, callable, vec![deadline])
        })();
        match token {
            Err(error) => out.settle(Err(error)),
            Ok(token) => token.subscribe(move |value| out.settle_shared(value)),
        }
    }
}
