//! Private wire records corresponding to apexomni.private.interface.ts.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};

// ts: WalletObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WalletObject {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "balance")]
    pub balance: String,
    #[serde(rename = "asset")]
    pub asset: String,
    #[serde(rename = "pendingDepositAmount")]
    pub pending_deposit_amount: String,
    #[serde(rename = "pendingWithdrawAmount")]
    pub pending_withdraw_amount: String,
    #[serde(rename = "pendingTransferOutAmount")]
    pub pending_transfer_out_amount: String,
    #[serde(rename = "pendingTransferInAmount")]
    pub pending_transfer_in_amount: String,
}
impl<'de> serde::Deserialize<'de> for WalletObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "userId")]
            user_id: String,
            #[serde(rename = "accountId")]
            account_id: String,
            #[serde(rename = "balance")]
            balance: String,
            #[serde(rename = "asset")]
            asset: String,
            #[serde(rename = "pendingDepositAmount")]
            pending_deposit_amount: String,
            #[serde(rename = "pendingWithdrawAmount")]
            pending_withdraw_amount: String,
            #[serde(rename = "pendingTransferOutAmount")]
            pending_transfer_out_amount: String,
            #[serde(rename = "pendingTransferInAmount")]
            pending_transfer_in_amount: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    user_id: fields.user_id,
                    account_id: fields.account_id,
                    balance: fields.balance,
                    asset: fields.asset,
                    pending_deposit_amount: fields.pending_deposit_amount,
                    pending_withdraw_amount: fields.pending_withdraw_amount,
                    pending_transfer_out_amount: fields.pending_transfer_out_amount,
                    pending_transfer_in_amount: fields.pending_transfer_in_amount,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WalletObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { user_id, account_id, balance, asset, pending_deposit_amount, pending_withdraw_amount, pending_transfer_out_amount, pending_transfer_in_amount });
    }
}

// ts: OpenPositionObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OpenPositionObject {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "maxSize")]
    pub max_size: String,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "exitPrice")]
    pub exit_price: String,
    #[serde(rename = "unrealizedPnl")]
    pub unrealized_pnl: String,
    #[serde(rename = "realizedPnl")]
    pub realized_pnl: String,
    #[serde(rename = "createdAt")]
    pub created_at: f64,
    #[serde(rename = "updatedTime")]
    pub updated_time: f64,
    #[serde(rename = "netFunding")]
    pub net_funding: String,
    #[serde(rename = "sumOpen")]
    pub sum_open: String,
    #[serde(rename = "sumClose")]
    pub sum_close: String,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "fundingFee")]
    pub funding_fee: String,
    #[serde(rename = "lightNumbers")]
    pub light_numbers: String,
    #[serde(rename = "customInitialMarginRate")]
    pub custom_initial_margin_rate: String,
}
impl<'de> serde::Deserialize<'de> for OpenPositionObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "userId")]
            user_id: String,
            #[serde(rename = "accountId")]
            account_id: String,
            #[serde(rename = "symbol")]
            symbol: String,
            #[serde(rename = "status")]
            status: String,
            #[serde(rename = "side")]
            side: String,
            #[serde(rename = "size")]
            size: String,
            #[serde(rename = "maxSize")]
            max_size: String,
            #[serde(rename = "entryPrice")]
            entry_price: String,
            #[serde(rename = "exitPrice")]
            exit_price: String,
            #[serde(rename = "unrealizedPnl")]
            unrealized_pnl: String,
            #[serde(rename = "realizedPnl")]
            realized_pnl: String,
            #[serde(rename = "createdAt")]
            created_at: f64,
            #[serde(rename = "updatedTime")]
            updated_time: f64,
            #[serde(rename = "netFunding")]
            net_funding: String,
            #[serde(rename = "sumOpen")]
            sum_open: String,
            #[serde(rename = "sumClose")]
            sum_close: String,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "fundingFee")]
            funding_fee: String,
            #[serde(rename = "lightNumbers")]
            light_numbers: String,
            #[serde(rename = "customInitialMarginRate")]
            custom_initial_margin_rate: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    user_id: fields.user_id,
                    account_id: fields.account_id,
                    symbol: fields.symbol,
                    status: fields.status,
                    side: fields.side,
                    size: fields.size,
                    max_size: fields.max_size,
                    entry_price: fields.entry_price,
                    exit_price: fields.exit_price,
                    unrealized_pnl: fields.unrealized_pnl,
                    realized_pnl: fields.realized_pnl,
                    created_at: fields.created_at,
                    updated_time: fields.updated_time,
                    net_funding: fields.net_funding,
                    sum_open: fields.sum_open,
                    sum_close: fields.sum_close,
                    fee: fields.fee,
                    funding_fee: fields.funding_fee,
                    light_numbers: fields.light_numbers,
                    custom_initial_margin_rate: fields.custom_initial_margin_rate,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for OpenPositionObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { user_id, account_id, symbol, status, side, size, max_size, entry_price, exit_price, unrealized_pnl, realized_pnl, created_at, updated_time, net_funding, sum_open, sum_close, fee, funding_fee, light_numbers, custom_initial_margin_rate });
    }
}

// ts: AccountsItem
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AccountsItem {
    #[serde(rename = "createdAt")]
    pub created_at: f64,
    #[serde(rename = "takerFeeRate")]
    pub taker_fee_rate: String,
    #[serde(rename = "makerFeeRate")]
    pub maker_fee_rate: String,
    #[serde(rename = "minInitialMarginRate")]
    pub min_initial_margin_rate: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "token")]
    pub token: String,
    #[serde(rename = "unrealizePnlPriceType")]
    pub unrealize_pnl_price_type: String,
}
impl<'de> serde::Deserialize<'de> for AccountsItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "createdAt")]
            created_at: f64,
            #[serde(rename = "takerFeeRate")]
            taker_fee_rate: String,
            #[serde(rename = "makerFeeRate")]
            maker_fee_rate: String,
            #[serde(rename = "minInitialMarginRate")]
            min_initial_margin_rate: String,
            #[serde(rename = "status")]
            status: String,
            #[serde(rename = "token")]
            token: String,
            #[serde(rename = "unrealizePnlPriceType")]
            unrealize_pnl_price_type: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    created_at: fields.created_at,
                    taker_fee_rate: fields.taker_fee_rate,
                    maker_fee_rate: fields.maker_fee_rate,
                    min_initial_margin_rate: fields.min_initial_margin_rate,
                    status: fields.status,
                    token: fields.token,
                    unrealize_pnl_price_type: fields.unrealize_pnl_price_type,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for AccountsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { created_at, taker_fee_rate, maker_fee_rate, min_initial_margin_rate, status, token, unrealize_pnl_price_type });
    }
}

// ts: AccountObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AccountObject {
    #[serde(rename = "starkKey")]
    pub stark_key: String,
    #[serde(rename = "positionId")]
    pub position_id: String,
    #[serde(rename = "quoteBalance")]
    pub quote_balance: String,
    #[serde(rename = "pendingDeposits")]
    pub pending_deposits: String,
    #[serde(rename = "pendingWithdrawals")]
    pub pending_withdrawals: String,
    #[serde(rename = "createdAt")]
    pub created_at: f64,
    #[serde(rename = "accountNumber")]
    pub account_number: String,
    #[serde(rename = "adminSignature")]
    pub admin_signature: String,
    #[serde(rename = "ethereumAddress")]
    pub ethereum_address: String,
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "takerFeeRate")]
    pub taker_fee_rate: String,
    #[serde(rename = "makerFeeRate")]
    pub maker_fee_rate: String,
    #[serde(rename = "wallets")]
    pub wallets: Vec<WalletObject>,
    #[serde(rename = "openPositions")]
    pub open_positions: Vec<OpenPositionObject>,
    #[serde(rename = "accounts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounts: Option<Vec<AccountsItem>>,
}
impl<'de> serde::Deserialize<'de> for AccountObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "starkKey")]
            stark_key: String,
            #[serde(rename = "positionId")]
            position_id: String,
            #[serde(rename = "quoteBalance")]
            quote_balance: String,
            #[serde(rename = "pendingDeposits")]
            pending_deposits: String,
            #[serde(rename = "pendingWithdrawals")]
            pending_withdrawals: String,
            #[serde(rename = "createdAt")]
            created_at: f64,
            #[serde(rename = "accountNumber")]
            account_number: String,
            #[serde(rename = "adminSignature")]
            admin_signature: String,
            #[serde(rename = "ethereumAddress")]
            ethereum_address: String,
            #[serde(rename = "id")]
            id: String,
            #[serde(rename = "takerFeeRate")]
            taker_fee_rate: String,
            #[serde(rename = "makerFeeRate")]
            maker_fee_rate: String,
            #[serde(rename = "wallets")]
            wallets: Vec<WalletObject>,
            #[serde(rename = "openPositions")]
            open_positions: Vec<OpenPositionObject>,
            #[serde(rename = "accounts")]
            #[serde(default, deserialize_with = "deserialize_present")]
            accounts: Option<Vec<AccountsItem>>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    stark_key: fields.stark_key,
                    position_id: fields.position_id,
                    quote_balance: fields.quote_balance,
                    pending_deposits: fields.pending_deposits,
                    pending_withdrawals: fields.pending_withdrawals,
                    created_at: fields.created_at,
                    account_number: fields.account_number,
                    admin_signature: fields.admin_signature,
                    ethereum_address: fields.ethereum_address,
                    id: fields.id,
                    taker_fee_rate: fields.taker_fee_rate,
                    maker_fee_rate: fields.maker_fee_rate,
                    wallets: fields.wallets,
                    open_positions: fields.open_positions,
                    accounts: fields.accounts,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for AccountObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { stark_key, position_id, quote_balance, pending_deposits, pending_withdrawals, created_at, account_number, admin_signature, ethereum_address, id, taker_fee_rate, maker_fee_rate, wallets, open_positions, accounts });
    }
}

// ts: WorstPriceObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WorstPriceObject {
    #[serde(rename = "worstPrice")]
    pub worst_price: String,
    #[serde(rename = "bidOnePrice")]
    pub bid_one_price: String,
    #[serde(rename = "askOnePrice")]
    pub ask_one_price: String,
}
impl<'de> serde::Deserialize<'de> for WorstPriceObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "worstPrice")]
            worst_price: String,
            #[serde(rename = "bidOnePrice")]
            bid_one_price: String,
            #[serde(rename = "askOnePrice")]
            ask_one_price: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    worst_price: fields.worst_price,
                    bid_one_price: fields.bid_one_price,
                    ask_one_price: fields.ask_one_price,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WorstPriceObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { worst_price, bid_one_price, ask_one_price });
    }
}

// ts: FundingRateObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FundingRateObject {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "rate")]
    pub rate: String,
    #[serde(rename = "positionSize")]
    pub position_size: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "fundingTime")]
    pub funding_time: f64,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "fundingValue")]
    pub funding_value: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "transactionId")]
    pub transaction_id: String,
}
impl<'de> serde::Deserialize<'de> for FundingRateObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "id")]
            id: String,
            #[serde(rename = "symbol")]
            symbol: String,
            #[serde(rename = "rate")]
            rate: String,
            #[serde(rename = "positionSize")]
            position_size: String,
            #[serde(rename = "price")]
            price: String,
            #[serde(rename = "fundingTime")]
            funding_time: f64,
            #[serde(rename = "side")]
            side: String,
            #[serde(rename = "fundingValue")]
            funding_value: String,
            #[serde(rename = "status")]
            status: String,
            #[serde(rename = "transactionId")]
            transaction_id: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    id: fields.id,
                    symbol: fields.symbol,
                    rate: fields.rate,
                    position_size: fields.position_size,
                    price: fields.price,
                    funding_time: fields.funding_time,
                    side: fields.side,
                    funding_value: fields.funding_value,
                    status: fields.status,
                    transaction_id: fields.transaction_id,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FundingRateObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, symbol, rate, position_size, price, funding_time, side, funding_value, status, transaction_id });
    }
}

// ts: HistoricalPNLObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HistoricalPNLObject {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "exitPrice")]
    pub exit_price: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "totalPnl")]
    pub total_pnl: String,
    #[serde(rename = "createdAt")]
    pub created_at: f64,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "isLiquidate")]
    pub is_liquidate: bool,
    #[serde(rename = "isDeleverage")]
    pub is_deleverage: bool,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "closeSharedOpenFee")]
    pub close_shared_open_fee: String,
    #[serde(rename = "liquidateFee")]
    pub liquidate_fee: String,
}
impl<'de> serde::Deserialize<'de> for HistoricalPNLObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "symbol")]
            symbol: String,
            #[serde(rename = "size")]
            size: String,
            #[serde(rename = "exitPrice")]
            exit_price: String,
            #[serde(rename = "price")]
            price: String,
            #[serde(rename = "side")]
            side: String,
            #[serde(rename = "totalPnl")]
            total_pnl: String,
            #[serde(rename = "createdAt")]
            created_at: f64,
            #[serde(rename = "type")]
            r#type: String,
            #[serde(rename = "isLiquidate")]
            is_liquidate: bool,
            #[serde(rename = "isDeleverage")]
            is_deleverage: bool,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "closeSharedOpenFee")]
            close_shared_open_fee: String,
            #[serde(rename = "liquidateFee")]
            liquidate_fee: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    symbol: fields.symbol,
                    size: fields.size,
                    exit_price: fields.exit_price,
                    price: fields.price,
                    side: fields.side,
                    total_pnl: fields.total_pnl,
                    created_at: fields.created_at,
                    r#type: fields.r#type,
                    is_liquidate: fields.is_liquidate,
                    is_deleverage: fields.is_deleverage,
                    fee: fields.fee,
                    close_shared_open_fee: fields.close_shared_open_fee,
                    liquidate_fee: fields.liquidate_fee,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for HistoricalPNLObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, size, exit_price, price, side, total_pnl, created_at, r#type, is_liquidate, is_deleverage, fee, close_shared_open_fee, liquidate_fee });
    }
}

// ts: AccountBalanceObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AccountBalanceObject {
    #[serde(rename = "totalEquityValue")]
    pub total_equity_value: String,
    #[serde(rename = "availableBalance")]
    pub available_balance: String,
    #[serde(rename = "initialMargin")]
    pub initial_margin: String,
    #[serde(rename = "maintenanceMargin")]
    pub maintenance_margin: String,
}
impl<'de> serde::Deserialize<'de> for AccountBalanceObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "totalEquityValue")]
            total_equity_value: String,
            #[serde(rename = "availableBalance")]
            available_balance: String,
            #[serde(rename = "initialMargin")]
            initial_margin: String,
            #[serde(rename = "maintenanceMargin")]
            maintenance_margin: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    total_equity_value: fields.total_equity_value,
                    available_balance: fields.available_balance,
                    initial_margin: fields.initial_margin,
                    maintenance_margin: fields.maintenance_margin,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for AccountBalanceObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { total_equity_value, available_balance, initial_margin, maintenance_margin });
    }
}

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
