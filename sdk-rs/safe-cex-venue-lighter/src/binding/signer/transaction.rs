//! lighter-sdk TransactionClient: a JS class over the native
//! `transaction_client::TransactionClient`, whose HTTP and crypto
//! dependencies are the JS objects handed to its constructor.
use super::{Host, NativeSlots, codec};
use crate::{
    binding::{
        ops::{JsIterator, has_property, to_string},
        value::{to_signing, to_thrown},
    },
    http_transactions::L2CreateOrderTxInfo,
    transaction_client::{
        SendTxBatchResponse, TransactionApi, TransactionClient, TransactionClientConfig,
        TransactionRef,
    },
    transaction_promise::{NativePromise, ObjectPromise, Shared},
    transaction_signing::{
        CryptoCallResult, CryptoFailure, Fp5Callback, HostThrownValue, TransactionCrypto,
    },
};
use num_bigint::BigInt;
use safe_cex_exchange::js_runtime::{
    promise::{Promise, await_js},
    value::{Js, Thrown, arg},
};
use std::{cell::RefCell, rc::Rc};

/// `this.httpClient.<member>(...args)`, awaited.
struct JsTransactionApi {
    host: Host,
    client: Js,
}

impl JsTransactionApi {
    fn send<T: 'static>(
        &self,
        member: &str,
        args: Vec<Js>,
        map: impl FnOnce(Js) -> Result<T, Thrown> + 'static,
    ) -> NativePromise<T> {
        match self.client.call_method(member, args) {
            Ok(value) => self.host.awaited(value, map),
            Err(error) => {
                NativePromise::settled(self.host.microtasks.clone(), Err(to_signing(error)))
            }
        }
    }
}

fn text(value: Js) -> Result<Vec<u16>, Thrown> {
    match value {
        Js::String(s) => Ok(s.to_vec()),
        other => Err(Thrown::type_error(&format!(
            "sendRawTx must resolve to a string, received {}",
            other.type_of()
        ))),
    }
}

impl TransactionApi for JsTransactionApi {
    fn get_next_nonce(&self, account_index: &BigInt, api_key_index: f64) -> NativePromise<BigInt> {
        let args = vec![
            Js::BigInt(Rc::new(account_index.clone())),
            Js::Number(api_key_index),
        ];
        // `nonce + 1n` refuses every non-bigint nonce the client could cache.
        self.send("getNextNonce", args, |nonce| match nonce {
            Js::BigInt(n) => Ok((*n).clone()),
            _ => Err(Thrown::type_error(
                "Cannot mix BigInt and other types, use explicit conversions",
            )),
        })
    }
    fn send_raw_tx(&self, transaction: TransactionRef) -> NativePromise<Vec<u16>> {
        self.send(
            "sendRawTx",
            vec![codec::transaction_ref(&transaction)],
            text,
        )
    }
    fn send_raw_tx_batch(
        &self,
        transactions: Shared<Vec<Shared<L2CreateOrderTxInfo>>>,
    ) -> ObjectPromise<SendTxBatchResponse> {
        let batch = Js::array(
            transactions
                .borrow()
                .iter()
                .map(|tx| codec::transaction_ref(&TransactionRef::CreateOrder(tx.clone())))
                .collect(),
        );
        self.send("sendRawTxBatch", vec![batch], |response| {
            codec::send_tx_batch_response(&response).map(RefCell::new)
        })
    }
}

/// `this.crypto.<member>(...)` results: `{ error }` objects are failures.
struct JsCrypto {
    crypto: Js,
    fp5_callee: Rc<RefCell<Js>>,
    fp5: Box<Fp5Callback<'static>>,
}

fn thrown(error: Thrown) -> HostThrownValue {
    HostThrownValue::new(error)
}

// `typeof r === 'object' && 'error' in r` selects the `${r.error}` failure.
fn crypto_result(result: Result<Js, Thrown>) -> CryptoCallResult {
    let value = result.map_err(|e| CryptoFailure::Thrown(thrown(e)))?;
    let failure = |e: Thrown| CryptoFailure::Thrown(thrown(e));
    if value.type_of() == "object" {
        if matches!(value, Js::Null) {
            return Err(failure(Thrown::type_error(
                "Cannot use 'in' operator to search for 'error' in null",
            )));
        }
        if has_property(&value, &"error".encode_utf16().collect::<Vec<_>>()) {
            let message = value.get("error").and_then(|e| to_string(&e));
            return Err(CryptoFailure::ReturnedError(
                message.map_err(failure)?.to_vec(),
            ));
        }
    }
    to_string(&value).map(|s| s.to_vec()).map_err(failure)
}

impl JsCrypto {
    fn new(crypto: Js) -> Self {
        let fp5_callee = Rc::new(RefCell::new(Js::Undefined));
        let callee = fp5_callee.clone();
        let fp5: Box<Fp5Callback<'static>> = Box::new(move |input| {
            let callee = callee.borrow().clone();
            crypto_result(callee.call(Js::Undefined, vec![Js::utf16(input)]))
        });
        Self {
            crypto,
            fp5_callee,
            fp5,
        }
    }
    fn call(&self, member: &str, args: Vec<Js>) -> CryptoCallResult {
        crypto_result(self.crypto.call_method(member, args))
    }
}

impl TransactionCrypto for JsCrypto {
    fn is_ready(&self) -> Result<bool, HostThrownValue> {
        self.crypto
            .call_method("isReady", vec![])
            .map(|ready| ready.truthy())
            .map_err(thrown)
    }
    fn poseidon2_hash(&self, input: &[u16]) -> CryptoCallResult {
        self.call("poseidon2Hash", vec![Js::utf16(input)])
    }
    fn poseidon2_two_to_one(&self, left: &[u16], right: &[u16]) -> CryptoCallResult {
        self.call(
            "poseidon2HashTwoToOne",
            vec![Js::utf16(left), Js::utf16(right)],
        )
    }
    fn fp5_callback(&self) -> Result<Option<&Fp5Callback<'_>>, HostThrownValue> {
        let callee = self.crypto.get("poseidon2HashFp5").map_err(thrown)?;
        let present = callee.truthy();
        *self.fp5_callee.borrow_mut() = callee;
        Ok(present.then_some(self.fp5.as_ref()))
    }
    fn schnorr_sign(&self, input: &[u16], private_key: &[u16]) -> CryptoCallResult {
        self.call(
            "schnorrSign",
            vec![Js::utf16(input), Js::utf16(private_key)],
        )
    }
}

fn config_field<T>(
    config: &Js,
    key: &str,
    read: impl FnOnce(Js) -> Option<T>,
) -> Result<T, Thrown> {
    let value = config.get(key)?;
    let kind = value.type_of();
    read(value).ok_or_else(|| {
        Thrown::type_error(&format!(
            "TransactionClient config.{key} has unsupported type {kind}"
        ))
    })
}

// ts: TransactionClient.constructor
fn construct(host: &Host, config: &Js) -> Result<(Rc<TransactionClient>, Js), Thrown> {
    let http_client = config.get("httpClient")?;
    let crypto = config.get("crypto")?;
    let chain_id = config_field(config, "chainId", |v| v.as_f64())?;
    let private_key = config_field(config, "privateKey", |v| v.as_str().map(|s| s.to_vec()))?;
    let account_index = config_field(config, "accountIndex", |v| match v {
        Js::BigInt(n) => Some((*n).clone()),
        _ => None,
    })?;
    let api_key_index = config_field(config, "apiKeyIndex", |v| v.as_f64())?;
    let client = TransactionClient::new(TransactionClientConfig {
        http_client: Rc::new(JsTransactionApi {
            host: host.clone(),
            client: http_client.clone(),
        }),
        crypto: Rc::new(JsCrypto::new(crypto)),
        runtime: host.microtasks.clone(),
        chain_id,
        private_key,
        account_index,
        api_key_index,
    });
    Ok((client, http_client))
}

type Method = fn(&Host, &Rc<TransactionClient>, Js) -> Js;

// Parameter reads happen in the nonce continuation, never at invocation.
fn after_nonce(
    host: &Host,
    client: &Rc<TransactionClient>,
    params: Js,
    build: impl FnOnce(&TransactionClient, Js, BigInt) -> Result<Js, Thrown> + 'static,
) -> Js {
    let nonce = client.get_next_nonce(false);
    let client = client.clone();
    host.promise(nonce, move |nonce| build(&client, params, nonce.clone()))
}

fn build_order(client: &TransactionClient, params: &Js, nonce: BigInt) -> Result<Js, Thrown> {
    let (params, flags) = codec::create_order(params)?;
    let tx = client.create_order(&params, &nonce).map_err(to_thrown)?;
    Ok(codec::create_order_tx(&tx, &flags))
}

fn create_orders(host: &Host, client: &Rc<TransactionClient>, params: Js) -> Js {
    let iterator = match JsIterator::open(&params) {
        Ok(iterator) => Rc::new(iterator),
        Err(error) => return host.rejected(error),
    };
    let result = Promise::pending(&host.runtime);
    create_order_step(client.clone(), iterator, vec![], result.clone());
    Js::Promise(result)
}

fn create_order_step(
    client: Rc<TransactionClient>,
    iterator: Rc<JsIterator>,
    mut transactions: Vec<Js>,
    result: Promise,
) {
    let params = match iterator.step() {
        Ok(Some(params)) => params,
        Ok(None) => {
            result.settle(Ok(Js::array(transactions)));
            return;
        }
        Err(error) => {
            result.settle(Err(error));
            return;
        }
    };
    client.get_next_nonce(false).subscribe(move |nonce| {
        let built = nonce
            .map_err(to_thrown)
            .and_then(|nonce| build_order(&client, &params, (*nonce).clone()));
        match built {
            Ok(transaction) => {
                transactions.push(transaction);
                create_order_step(client, iterator, transactions, result);
            }
            Err(error) => {
                // IteratorClose preserves the original abrupt body completion.
                let closed = iterator.close();
                drop(closed);
                result.settle(Err(error));
            }
        }
    });
}

const METHODS: [(&str, Method); 9] = [
    // ts: TransactionClient.getCreateOrderTransaction
    ("getCreateOrderTransaction", |host, client, params| {
        after_nonce(host, client, params, |client, params, nonce| {
            build_order(client, &params, nonce)
        })
    }),
    // ts: TransactionClient.getCreateOrderTransactions
    ("getCreateOrderTransactions", create_orders),
    // ts: TransactionClient.getCreateGroupedOrdersTransaction
    (
        "getCreateGroupedOrdersTransaction",
        |host, client, params| {
            after_nonce(host, client, params, |client, params, nonce| {
                let (params, flags) = codec::create_grouped_orders(&params)?;
                let flags = Rc::new(flags);
                let compare = flags.clone();
                let tx = client
                    .grouped_orders(
                        params,
                        nonce,
                        Rc::new(move |a, b| compare[a].is_buy.strict_equals(&compare[b].is_buy)),
                    )
                    .map_err(to_thrown)?;
                Ok(codec::create_grouped_orders_tx(&tx, &flags))
            })
        },
    ),
    // ts: TransactionClient.getCancelOrderTransaction
    ("getCancelOrderTransaction", |host, client, params| {
        after_nonce(host, client, params, |client, params, nonce| {
            let tx = client
                .cancel_order(codec::cancel_order(&params)?, nonce)
                .map_err(to_thrown)?;
            Ok(codec::cancel_order_tx(&tx))
        })
    }),
    // ts: TransactionClient.getTransferTransaction
    ("getTransferTransaction", |host, client, params| {
        after_nonce(host, client, params, |client, params, nonce| {
            let tx = client
                .transfer(codec::transfer(&params)?, nonce)
                .map_err(to_thrown)?;
            Ok(codec::transfer_tx(&tx))
        })
    }),
    // ts: TransactionClient.getUpdateLeverageTransaction
    ("getUpdateLeverageTransaction", |host, client, params| {
        after_nonce(host, client, params, |client, params, nonce| {
            let tx = client
                .update_leverage(codec::update_leverage(&params)?, nonce)
                .map_err(to_thrown)?;
            Ok(codec::update_leverage_tx(&tx))
        })
    }),
    // ts: TransactionClient.getApproveIntegratorTransaction
    ("getApproveIntegratorTransaction", |host, client, params| {
        let build = |client: &TransactionClient, params: Js, nonce: BigInt| {
            let (params, l1_signature) = codec::approve_integrator(&params, Some(nonce.clone()))?;
            let tx = client
                .approve_integrator(params, nonce)
                .map_err(to_thrown)?;
            Ok(codec::approve_integrator_tx(&tx, &l1_signature))
        };
        match codec::approve_nonce(&params) {
            Ok(Some(nonce)) => match build(client, params, nonce) {
                Ok(tx) => Js::Promise(Promise::settled(&host.runtime, Ok(tx))),
                Err(error) => host.rejected(error),
            },
            Ok(None) => after_nonce(host, client, params, build),
            Err(error) => host.rejected(error),
        }
    }),
    // ts: TransactionClient.getAuthToken
    (
        "getAuthToken",
        |host, client, deadline| match codec::deadline(&deadline) {
            Ok(deadline) => host.promise(client.get_auth_token(deadline), |token| {
                Ok(Js::utf16(token))
            }),
            Err(error) => host.rejected(error),
        },
    ),
    // ts: TransactionClient.resetNonce (synchronous)
    ("resetNonce", |_, client, _| {
        client.reset_nonce();
        Js::Undefined
    }),
];

const SUBMITS: [(&str, &str); 7] = [
    ("submitCreateOrder", "getCreateOrderTransaction"),
    ("submitCreateOrders", "getCreateOrderTransactions"),
    (
        "submitCreateGroupedOrders",
        "getCreateGroupedOrdersTransaction",
    ),
    ("submitCancelOrder", "getCancelOrderTransaction"),
    ("submitTransfer", "getTransferTransaction"),
    ("submitUpdateLeverage", "getUpdateLeverageTransaction"),
    ("submitApproveIntegrator", "getApproveIntegratorTransaction"),
];

fn submit(host: &Host, this: Js, params: Js, getter: &str) -> Js {
    let built = match this.call_method(getter, vec![params]) {
        Ok(value) => value,
        Err(error) => return host.rejected(error),
    };
    let runtime = host.runtime.clone();
    let batch = getter == "getCreateOrderTransactions";
    let pending = await_js(&runtime, built);
    Js::Promise(Promise::spawn(
        &host.runtime,
        Box::pin(async move {
            let transaction = pending.await?;
            let api = this.get("httpClient")?;
            let sent = api.call_method(
                if batch { "sendRawTxBatch" } else { "sendRawTx" },
                vec![transaction],
            )?;
            let response = await_js(&runtime, sent).await?;
            if batch {
                response.get("txHashes")
            } else {
                Ok(response)
            }
        }),
    ))
}

pub(super) fn install(host: &Host) -> Result<(), Thrown> {
    let slots: Rc<NativeSlots<TransactionClient>> = Rc::default();
    let prototype = Js::object(vec![]);
    for (name, body) in METHODS {
        let (host, slots) = (host.clone(), slots.clone());
        let method = Js::function(name, move |this, a| match slots.get(&this) {
            Some(client) => Ok(body(&host, &client, arg(&a, 0))),
            None if name == "resetNonce" => Err(incompatible(name)),
            None => Ok(host.rejected(incompatible(name))),
        });
        prototype.define(name, method)?;
    }
    for (name, getter) in SUBMITS {
        let host = host.clone();
        prototype.define(
            name,
            Js::function(name, move |this, args| {
                Ok(submit(&host, this, arg(&args, 0), getter))
            }),
        )?;
    }
    let ctor_host = host.clone();
    let class = Js::constructor("TransactionClient", None, move |target, a| {
        let instance = target.allocate_instance()?;
        let (client, http_client) = construct(&ctor_host, &arg(&a, 0))?;
        instance.define("httpClient", http_client)?;
        slots.bind(&instance, client)?;
        Ok(instance)
    });
    class.define("prototype", prototype)?;
    host.define_global("TransactionClient", class)
}

fn incompatible(name: &str) -> Thrown {
    Thrown::type_error(&format!(
        "TransactionClient.prototype.{name} called on an incompatible receiver"
    ))
}
