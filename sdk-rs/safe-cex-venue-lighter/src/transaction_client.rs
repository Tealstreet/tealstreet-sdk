//! Complete TransactionClient orchestration at typed host boundaries. The HTTP
//! implementation and cryptographic arithmetic remain separate dependencies.
use super::{
    http_transactions::*, transaction_builders::*, transaction_client_types::*,
    transaction_promise::*, transaction_signing::*,
};
use num_bigint::BigInt;
use std::{cell::RefCell, rc::Rc};
#[derive(Debug, Clone, PartialEq)]
pub enum VolumeQuotaRemaining {
    Number(f64),
    String(Vec<u16>),
}
// ts: http-types::SendTxBatchResponse
#[derive(Debug, Clone, PartialEq)]
pub struct SendTxBatchResponse {
    pub tx_hashes: Shared<Vec<Vec<u16>>>,
    pub predicted_execution_time_ms: Option<f64>,
    pub volume_quota_remaining: Option<VolumeQuotaRemaining>,
}
#[derive(Clone)]
pub enum TransactionRef {
    CreateOrder(Shared<L2CreateOrderTxInfo>),
    CreateGroupedOrders(Shared<L2CreateGroupedOrdersTxInfo>),
    CancelOrder(Shared<L2CancelOrderTxInfo>),
    Transfer(Shared<L2TransferTxInfo>),
    UpdateLeverage(Shared<L2UpdateLeverageTxInfo>),
    ApproveIntegrator(Shared<L2ApproveIntegratorTxInfo>),
}
pub trait TransactionApi {
    fn get_next_nonce(&self, account_index: &BigInt, api_key_index: f64) -> NativePromise<BigInt>;
    fn send_raw_tx(&self, transaction: TransactionRef) -> NativePromise<Vec<u16>>;
    fn send_raw_tx_batch(
        &self,
        transactions: Shared<Vec<Shared<L2CreateOrderTxInfo>>>,
    ) -> ObjectPromise<SendTxBatchResponse>;
}
pub struct TransactionClientConfig {
    pub http_client: Rc<dyn TransactionApi>,
    pub crypto: Rc<dyn TransactionCrypto>,
    pub runtime: Rc<dyn TransactionRuntime>,
    pub chain_id: f64,
    pub private_key: Vec<u16>,
    pub account_index: BigInt,
    pub api_key_index: f64,
}
// ts: TransactionClient
pub struct TransactionClient {
    config: TransactionClientConfig,
    cached_nonce: RefCell<Option<BigInt>>,
    nonce_lock: RefCell<NativePromise<()>>,
}
impl TransactionClient {
    // ts: TransactionClient.constructor
    pub fn new(config: TransactionClientConfig) -> Rc<Self> {
        let nonce_lock = NativePromise::settled(config.runtime.clone(), Ok(()));
        Rc::new(Self {
            config,
            cached_nonce: RefCell::new(None),
            nonce_lock: RefCell::new(nonce_lock),
        })
    }
    fn context<'a>(&'a self, clock: &'a dyn Fn() -> f64) -> TransactionBuildContext<'a> {
        TransactionBuildContext {
            chain_id: self.config.chain_id,
            account_index: &self.config.account_index,
            api_key_index: self.config.api_key_index,
            now_millis: clock,
        }
    }
    // ts: TransactionClient#getNextNonce
    pub fn get_next_nonce(self: &Rc<Self>, force_refresh: bool) -> NativePromise<BigInt> {
        let (next_lock, release) = NativePromise::pending(self.config.runtime.clone());
        let previous_lock = self.nonce_lock.replace(next_lock);
        let (promise, resolver) = NativePromise::pending(self.config.runtime.clone());
        let client = self.clone();
        previous_lock.subscribe(move |_| {
            if !force_refresh {
                let cached = client.cached_nonce.borrow().clone();
                if let Some(nonce) = cached {
                    *client.cached_nonce.borrow_mut() = Some(&nonce + 1);
                    release.settle(Ok(()));
                    resolver.settle(Ok(nonce));
                    return;
                }
            }
            let fetched = client
                .config
                .http_client
                .get_next_nonce(&client.config.account_index, client.config.api_key_index);
            fetched.subscribe(move |result| {
                if let Ok(nonce) = &result {
                    *client.cached_nonce.borrow_mut() = Some(nonce.as_ref() + 1);
                }
                // finally resolves the lock before the async function settles.
                release.settle(Ok(()));
                resolver.settle_shared(result);
            });
        });
        promise
    }
    fn after_nonce<T: Clone + 'static>(
        self: &Rc<Self>,
        next: NativePromise<BigInt>,
        build: impl FnOnce(&Self, BigInt) -> SigningResult<T> + 'static,
    ) -> ObjectPromise<T> {
        let (promise, resolver) = NativePromise::pending(self.config.runtime.clone());
        let client = self.clone();
        next.subscribe(move |result| {
            resolver.settle(
                result
                    .and_then(|nonce| build(&client, nonce.as_ref().clone()))
                    .map(RefCell::new),
            )
        });
        promise
    }
    // ts: TransactionClient.getCreateOrderTransaction
    pub fn get_create_order_transaction(
        self: &Rc<Self>,
        params: CreateOrderParams,
    ) -> ObjectPromise<L2CreateOrderTxInfo> {
        self.after_nonce(self.get_next_nonce(false), move |client, nonce| {
            client.create_order(&params, &nonce)
        })
    }
    fn create_order(
        &self,
        params: &CreateOrderParams,
        nonce: &BigInt,
    ) -> SigningResult<L2CreateOrderTxInfo> {
        let clock = || self.config.runtime.now_millis();
        build_create_order_transaction(
            &self.context(&clock),
            self.config.crypto.as_ref(),
            &self.config.private_key,
            params,
            nonce,
        )
    }
    // ts: TransactionClient.getCreateOrderTransactions
    pub fn get_create_order_transactions(
        self: &Rc<Self>,
        params: Vec<CreateOrderParams>,
    ) -> ObjectPromise<Vec<Shared<L2CreateOrderTxInfo>>> {
        let (promise, resolver) = NativePromise::pending(self.config.runtime.clone());
        if params.is_empty() {
            resolver.settle(Ok(RefCell::new(vec![])));
        } else {
            self.create_order_step(Rc::new(params), 0, vec![], resolver);
        }
        promise
    }
    fn create_order_step(
        self: &Rc<Self>,
        params: Rc<Vec<CreateOrderParams>>,
        index: usize,
        mut transactions: Vec<Shared<L2CreateOrderTxInfo>>,
        resolver: ObjectResolver<Vec<Shared<L2CreateOrderTxInfo>>>,
    ) {
        let client = self.clone();
        self.get_next_nonce(false).subscribe(move |result| {
            match result.and_then(|nonce| client.create_order(&params[index], nonce.as_ref())) {
                Err(error) => resolver.settle(Err(error)),
                Ok(transaction) => {
                    transactions.push(shared(transaction));
                    if index + 1 == params.len() {
                        resolver.settle(Ok(RefCell::new(transactions)));
                    } else {
                        client.create_order_step(params, index + 1, transactions, resolver);
                    }
                }
            }
        });
    }
    // ts: TransactionClient.getCreateGroupedOrdersTransaction
    pub fn get_create_grouped_orders_transaction(
        self: &Rc<Self>,
        params: CreateGroupedOrdersParams,
    ) -> ObjectPromise<L2CreateGroupedOrdersTxInfo> {
        let sides = params
            .orders
            .iter()
            .map(|order| order.is_buy)
            .collect::<Vec<_>>();
        self.get_create_grouped_orders_with_side_comparison(
            params,
            Rc::new(move |a, b| sides[a] == sides[b]),
        )
    }

    pub(crate) fn get_create_grouped_orders_with_side_comparison(
        self: &Rc<Self>,
        params: CreateGroupedOrdersParams,
        same_side: Rc<dyn Fn(usize, usize) -> bool>,
    ) -> ObjectPromise<L2CreateGroupedOrdersTxInfo> {
        self.after_nonce(self.get_next_nonce(false), move |client, nonce| {
            let clock = || client.config.runtime.now_millis();
            let payload = build_grouped_orders_with_side_comparison(
                &client.context(&clock),
                client.config.crypto.as_ref(),
                &params,
                &nonce,
                same_side.as_ref(),
            )?;
            let signed = sign_create_order_transaction(
                client.config.crypto.as_ref(),
                &client.config.private_key,
                &payload.message_hex,
                payload.l2_attributes_message_hex.as_deref(),
            )?;
            Ok(L2CreateGroupedOrdersTxInfo {
                header: client.header(
                    payload.message_hex,
                    signed.signature,
                    nonce,
                    Some(payload.expired_at),
                ),
                grouped_order_details: GroupedOrderDetails {
                    grouping_type: params.grouping_type,
                    orders: payload.orders,
                    integrator_account_index: payload.integrator_account_index,
                    integrator_maker_fee: payload.integrator_maker_fee,
                    integrator_taker_fee: payload.integrator_taker_fee,
                },
            })
        })
    }
    fn header(
        &self,
        data: Vec<u16>,
        signature: Vec<u16>,
        nonce: BigInt,
        expired_at: Option<BigInt>,
    ) -> TransactionHeader {
        TransactionHeader {
            data,
            signature,
            account_index: self.config.account_index.clone(),
            api_key_index: self.config.api_key_index,
            nonce,
            expired_at,
        }
    }
    fn sign(&self, data: &str) -> SigningResult<SignedTransactionData> {
        sign_transaction(
            self.config.crypto.as_ref(),
            &self.config.private_key,
            &data.encode_utf16().collect::<Vec<_>>(),
        )
    }
    // ts: TransactionClient.getCancelOrderTransaction
    pub fn get_cancel_order_transaction(
        self: &Rc<Self>,
        params: CancelOrderParams,
    ) -> ObjectPromise<L2CancelOrderTxInfo> {
        self.after_nonce(self.get_next_nonce(false), move |client, nonce| {
            let clock = || client.config.runtime.now_millis();
            let payload = build_cancel_order_data(&client.context(&clock), &params, &nonce)?;
            let signed = client.sign(&payload.message_hex)?;
            Ok(L2CancelOrderTxInfo {
                header: client.header(
                    payload.message_hex.encode_utf16().collect(),
                    signed.signature,
                    nonce,
                    Some(payload.expired_at.clone()),
                ),
                cancel_details: CancelDetails {
                    order_id: params.order_id,
                    market_id: params.market_id,
                    expired_at: Some(payload.expired_at),
                },
            })
        })
    }
    // ts: TransactionClient.getTransferTransaction
    pub fn get_transfer_transaction(
        self: &Rc<Self>,
        params: TransferParams,
    ) -> ObjectPromise<L2TransferTxInfo> {
        self.after_nonce(self.get_next_nonce(false), move |client, nonce| {
            let clock = || client.config.runtime.now_millis();
            let data = build_transfer_data(&client.context(&clock), &params, &nonce);
            let signed = client.sign(&data)?;
            Ok(L2TransferTxInfo {
                header: client.header(data.encode_utf16().collect(), signed.signature, nonce, None),
                transfer_details: TransferDetails {
                    to_account_index: params.to_account_index,
                    amount: params.amount,
                    token_id: params.token_id.unwrap_or(0.0),
                },
            })
        })
    }
    // ts: TransactionClient.getUpdateLeverageTransaction
    pub fn get_update_leverage_transaction(
        self: &Rc<Self>,
        params: UpdateLeverageParams,
    ) -> ObjectPromise<L2UpdateLeverageTxInfo> {
        self.after_nonce(self.get_next_nonce(false), move |client, nonce| {
            let clock = || client.config.runtime.now_millis();
            let payload = build_update_leverage_data(&client.context(&clock), &params, &nonce)?;
            let signed = client.sign(&payload.message_hex)?;
            Ok(L2UpdateLeverageTxInfo {
                header: client.header(
                    payload.message_hex.encode_utf16().collect(),
                    signed.signature,
                    nonce,
                    Some(payload.expired_at.clone()),
                ),
                leverage_details: LeverageDetails {
                    market_id: params.market_id,
                    initial_margin_fraction: payload.initial_margin_fraction,
                    margin_mode: params.margin_mode,
                    expired_at: Some(payload.expired_at),
                },
            })
        })
    }
    fn approve_integrator(
        &self,
        params: ApproveIntegratorParams,
        nonce: BigInt,
    ) -> SigningResult<L2ApproveIntegratorTxInfo> {
        let clock = || self.config.runtime.now_millis();
        let payload = build_approve_integrator_data(&self.context(&clock), &params, &nonce)?;
        let signed = self.sign(&payload.message_hex)?;
        Ok(L2ApproveIntegratorTxInfo {
            header: self.header(
                payload.message_hex.encode_utf16().collect(),
                signed.signature,
                nonce,
                Some(payload.expired_at.clone()),
            ),
            approve_integrator_details: ApproveIntegratorDetails {
                integrator_account_index: params.integrator_account_index,
                max_perps_taker_fee: params.max_perps_taker_fee,
                max_perps_maker_fee: params.max_perps_maker_fee,
                max_spot_taker_fee: params.max_spot_taker_fee,
                max_spot_maker_fee: params.max_spot_maker_fee,
                approval_expiry: params.approval_expiry,
                expired_at: Some(payload.expired_at),
                l1_signature: params.l1_signature,
            },
        })
    }
    // ts: TransactionClient.getApproveIntegratorTransaction
    pub fn get_approve_integrator_transaction(
        self: &Rc<Self>,
        params: ApproveIntegratorParams,
    ) -> ObjectPromise<L2ApproveIntegratorTxInfo> {
        if let Some(nonce) = params.nonce.clone() {
            NativePromise::settled(
                self.config.runtime.clone(),
                self.approve_integrator(params, nonce).map(RefCell::new),
            )
        } else {
            self.after_nonce(self.get_next_nonce(false), move |client, nonce| {
                client.approve_integrator(params, nonce)
            })
        }
    }
    // ts: TransactionClient.getAuthToken
    pub fn get_auth_token(&self, deadline: BigInt) -> NativePromise<Vec<u16>> {
        let clock = || self.config.runtime.now_millis();
        NativePromise::settled(
            self.config.runtime.clone(),
            get_auth_token_with_encoding(
                &self.context(&clock),
                self.config.crypto.as_ref(),
                &self.config.private_key,
                &deadline,
                || self.config.runtime.auth_token_encoding(),
            ),
        )
    }
    // ts: TransactionClient.resetNonce
    pub fn cached_nonce(&self) -> Option<BigInt> {
        self.cached_nonce.borrow().clone()
    }
    pub fn reset_nonce(&self) {
        *self.cached_nonce.borrow_mut() = None;
    }
    pub fn get_account_index(&self) -> BigInt {
        self.config.account_index.clone()
    }
    pub fn get_api_key_index(&self) -> f64 {
        self.config.api_key_index
    }
    pub fn get_chain_id(&self) -> f64 {
        self.config.chain_id
    }
    fn submit<T: Clone + 'static>(
        self: &Rc<Self>,
        transaction: ObjectPromise<T>,
        into_any: impl FnOnce(Shared<T>) -> TransactionRef + 'static,
    ) -> NativePromise<Vec<u16>> {
        let (promise, resolver) = NativePromise::pending(self.config.runtime.clone());
        let client = self.clone();
        transaction.subscribe(move |result| match result {
            Err(error) => resolver.settle(Err(error)),
            Ok(transaction) => client
                .config
                .http_client
                .send_raw_tx(into_any(transaction))
                .subscribe(move |result| resolver.settle_shared(result)),
        });
        promise
    }
    // ts: TransactionClient.submitCreateOrder
    pub fn submit_create_order(
        self: &Rc<Self>,
        params: CreateOrderParams,
    ) -> NativePromise<Vec<u16>> {
        self.submit(
            self.get_create_order_transaction(params),
            TransactionRef::CreateOrder,
        )
    }
    // ts: TransactionClient.submitCreateGroupedOrders
    pub fn submit_create_grouped_orders(
        self: &Rc<Self>,
        params: CreateGroupedOrdersParams,
    ) -> NativePromise<Vec<u16>> {
        self.submit(
            self.get_create_grouped_orders_transaction(params),
            TransactionRef::CreateGroupedOrders,
        )
    }
    // ts: TransactionClient.submitCancelOrder
    pub fn submit_cancel_order(
        self: &Rc<Self>,
        params: CancelOrderParams,
    ) -> NativePromise<Vec<u16>> {
        self.submit(
            self.get_cancel_order_transaction(params),
            TransactionRef::CancelOrder,
        )
    }
    // ts: TransactionClient.submitTransfer
    pub fn submit_transfer(self: &Rc<Self>, params: TransferParams) -> NativePromise<Vec<u16>> {
        self.submit(
            self.get_transfer_transaction(params),
            TransactionRef::Transfer,
        )
    }
    // ts: TransactionClient.submitUpdateLeverage
    pub fn submit_update_leverage(
        self: &Rc<Self>,
        params: UpdateLeverageParams,
    ) -> NativePromise<Vec<u16>> {
        self.submit(
            self.get_update_leverage_transaction(params),
            TransactionRef::UpdateLeverage,
        )
    }
    // ts: TransactionClient.submitApproveIntegrator
    pub fn submit_approve_integrator(
        self: &Rc<Self>,
        params: ApproveIntegratorParams,
    ) -> NativePromise<Vec<u16>> {
        self.submit(
            self.get_approve_integrator_transaction(params),
            TransactionRef::ApproveIntegrator,
        )
    }
    // ts: TransactionClient.submitCreateOrders
    pub fn submit_create_orders(
        self: &Rc<Self>,
        params: Vec<CreateOrderParams>,
    ) -> ObjectPromise<Vec<Vec<u16>>> {
        let (promise, resolver) = NativePromise::pending(self.config.runtime.clone());
        let client = self.clone();
        self.get_create_order_transactions(params)
            .subscribe(move |result| match result {
                Err(error) => resolver.settle(Err(error)),
                Ok(transactions) => client
                    .config
                    .http_client
                    .send_raw_tx_batch(transactions)
                    .subscribe(move |result| {
                        resolver.settle_shared(
                            result.map(|response| response.borrow().tx_hashes.clone()),
                        )
                    }),
            });
        promise
    }
}
