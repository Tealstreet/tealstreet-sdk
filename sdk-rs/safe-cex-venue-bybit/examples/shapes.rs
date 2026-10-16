use safe_cex_venue_bybit::{rate_limits::*, types::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::io::{self, Read};
fn round_trip<T: DeserializeOwned + Serialize + NativeIdentity>(value: Value) -> Value {
    match serde_json::from_value::<T>(value.clone()) {
        Ok(decoded) => {
            decoded.check_identity(&value);
            json!({"ok":serde_json::to_value(decoded).unwrap()})
        }
        Err(_) => json!({"rejected":true}),
    }
}
trait NativeIdentity {
    fn check_identity(&self, wire: &Value);
}
impl NativeIdentity for BybitApiKeyInfo {
    fn check_identity(&self, wire: &Value) {
        for key in wire.as_object().unwrap().keys() {
            assert!(
                [
                    "id",
                    "note",
                    "apiKey",
                    "readOnly",
                    "secret",
                    "ips",
                    "type",
                    "deadlineDay",
                    "expiredAt",
                    "createdAt",
                    "unified",
                    "uta",
                    "userID",
                    "inviterID",
                    "vipLevel",
                    "mktMakerLevel",
                    "affiliateID",
                    "rsaPublicKey",
                    "isMaster",
                    "parentUid",
                    "permissions"
                ]
                .contains(&key.as_str()),
                "unobserved native field BybitApiKeyInfo.{key}"
            );
        }
        assert_eq!(
            serde_json::to_value(&self.id).unwrap(),
            wire["id"],
            "native field identity BybitApiKeyInfo.id"
        );
        assert_eq!(
            serde_json::to_value(&self.note).unwrap(),
            wire["note"],
            "native field identity BybitApiKeyInfo.note"
        );
        assert_eq!(
            serde_json::to_value(&self.api_key).unwrap(),
            wire["apiKey"],
            "native field identity BybitApiKeyInfo.api_key"
        );
        assert_eq!(
            self.read_only.to_bits(),
            wire["readOnly"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.read_only"
        );
        assert_eq!(
            serde_json::to_value(&self.secret).unwrap(),
            wire["secret"],
            "native field identity BybitApiKeyInfo.secret"
        );
        assert_eq!(
            serde_json::to_value(&self.ips).unwrap(),
            wire["ips"],
            "native field identity BybitApiKeyInfo.ips"
        );
        assert_eq!(
            self.r#type.to_bits(),
            wire["type"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.r#type"
        );
        assert_eq!(
            self.deadline_day.to_bits(),
            wire["deadlineDay"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.deadline_day"
        );
        assert_eq!(
            serde_json::to_value(&self.expired_at).unwrap(),
            wire["expiredAt"],
            "native field identity BybitApiKeyInfo.expired_at"
        );
        assert_eq!(
            serde_json::to_value(&self.created_at).unwrap(),
            wire["createdAt"],
            "native field identity BybitApiKeyInfo.created_at"
        );
        assert_eq!(
            self.unified.to_bits(),
            wire["unified"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.unified"
        );
        assert_eq!(
            self.uta.to_bits(),
            wire["uta"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.uta"
        );
        assert_eq!(
            self.user_i_d.to_bits(),
            wire["userID"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.user_i_d"
        );
        assert_eq!(
            self.inviter_i_d.to_bits(),
            wire["inviterID"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.inviter_i_d"
        );
        assert_eq!(
            serde_json::to_value(&self.vip_level).unwrap(),
            wire["vipLevel"],
            "native field identity BybitApiKeyInfo.vip_level"
        );
        assert_eq!(
            serde_json::to_value(&self.mkt_maker_level).unwrap(),
            wire["mktMakerLevel"],
            "native field identity BybitApiKeyInfo.mkt_maker_level"
        );
        assert_eq!(
            self.affiliate_i_d.to_bits(),
            wire["affiliateID"].as_f64().unwrap().to_bits(),
            "native field identity BybitApiKeyInfo.affiliate_i_d"
        );
        assert_eq!(
            serde_json::to_value(&self.rsa_public_key).unwrap(),
            wire["rsaPublicKey"],
            "native field identity BybitApiKeyInfo.rsa_public_key"
        );
        assert_eq!(
            serde_json::to_value(self.is_master).unwrap(),
            wire["isMaster"],
            "native field identity BybitApiKeyInfo.is_master"
        );
        assert_eq!(
            serde_json::to_value(&self.parent_uid).unwrap(),
            wire["parentUid"],
            "native field identity BybitApiKeyInfo.parent_uid"
        );
        self.permissions.check_identity(&wire["permissions"]);
    }
}
impl NativeIdentity for BybitApiKeyPermissions {
    fn check_identity(&self, wire: &Value) {
        for key in wire.as_object().unwrap().keys() {
            assert!(
                [
                    "ContractTrade",
                    "Spot",
                    "Wallet",
                    "Options",
                    "Derivatives",
                    "CopyTrading",
                    "BlockTrade",
                    "Exchange",
                    "NFT"
                ]
                .contains(&key.as_str()),
                "unobserved native field BybitApiKeyPermissions.{key}"
            );
        }
        assert_eq!(
            serde_json::to_value(&self.contract_trade).unwrap(),
            wire["ContractTrade"],
            "native field identity BybitApiKeyPermissions.contract_trade"
        );
        assert_eq!(
            serde_json::to_value(&self.spot).unwrap(),
            wire["Spot"],
            "native field identity BybitApiKeyPermissions.spot"
        );
        assert_eq!(
            serde_json::to_value(&self.wallet).unwrap(),
            wire["Wallet"],
            "native field identity BybitApiKeyPermissions.wallet"
        );
        assert_eq!(
            serde_json::to_value(&self.options).unwrap(),
            wire["Options"],
            "native field identity BybitApiKeyPermissions.options"
        );
        assert_eq!(
            serde_json::to_value(&self.derivatives).unwrap(),
            wire["Derivatives"],
            "native field identity BybitApiKeyPermissions.derivatives"
        );
        assert_eq!(
            serde_json::to_value(&self.copy_trading).unwrap(),
            wire["CopyTrading"],
            "native field identity BybitApiKeyPermissions.copy_trading"
        );
        assert_eq!(
            serde_json::to_value(&self.block_trade).unwrap(),
            wire["BlockTrade"],
            "native field identity BybitApiKeyPermissions.block_trade"
        );
        assert_eq!(
            serde_json::to_value(&self.exchange).unwrap(),
            wire["Exchange"],
            "native field identity BybitApiKeyPermissions.exchange"
        );
        assert_eq!(
            serde_json::to_value(&self.n_f_t).unwrap(),
            wire["NFT"],
            "native field identity BybitApiKeyPermissions.n_f_t"
        );
    }
}
impl NativeIdentity for BybitTransactionRecord {
    fn check_identity(&self, wire: &Value) {
        for key in wire.as_object().unwrap().keys() {
            assert!(
                [
                    "symbol",
                    "category",
                    "side",
                    "transactionTime",
                    "type",
                    "qty",
                    "size",
                    "currency",
                    "tradePrice",
                    "funding",
                    "fee",
                    "cashFlow",
                    "change",
                    "cashBalance",
                    "feeRate",
                    "bonusChange",
                    "tradeId",
                    "orderId",
                    "orderLinkId"
                ]
                .contains(&key.as_str()),
                "unobserved native field BybitTransactionRecord.{key}"
            );
        }
        assert_eq!(
            serde_json::to_value(&self.symbol).unwrap(),
            wire["symbol"],
            "native field identity BybitTransactionRecord.symbol"
        );
        assert_eq!(
            serde_json::to_value(&self.category).unwrap(),
            wire["category"],
            "native field identity BybitTransactionRecord.category"
        );
        assert_eq!(
            serde_json::to_value(&self.side).unwrap(),
            wire["side"],
            "native field identity BybitTransactionRecord.side"
        );
        assert_eq!(
            serde_json::to_value(&self.transaction_time).unwrap(),
            wire["transactionTime"],
            "native field identity BybitTransactionRecord.transaction_time"
        );
        assert_eq!(
            serde_json::to_value(self.r#type).unwrap(),
            wire["type"],
            "native field identity BybitTransactionRecord.r#type"
        );
        assert_eq!(
            serde_json::to_value(&self.qty).unwrap(),
            wire["qty"],
            "native field identity BybitTransactionRecord.qty"
        );
        assert_eq!(
            serde_json::to_value(&self.size).unwrap(),
            wire["size"],
            "native field identity BybitTransactionRecord.size"
        );
        assert_eq!(
            serde_json::to_value(&self.currency).unwrap(),
            wire["currency"],
            "native field identity BybitTransactionRecord.currency"
        );
        assert_eq!(
            serde_json::to_value(&self.trade_price).unwrap(),
            wire["tradePrice"],
            "native field identity BybitTransactionRecord.trade_price"
        );
        assert_eq!(
            serde_json::to_value(&self.funding).unwrap(),
            wire["funding"],
            "native field identity BybitTransactionRecord.funding"
        );
        assert_eq!(
            serde_json::to_value(&self.fee).unwrap(),
            wire["fee"],
            "native field identity BybitTransactionRecord.fee"
        );
        assert_eq!(
            serde_json::to_value(&self.cash_flow).unwrap(),
            wire["cashFlow"],
            "native field identity BybitTransactionRecord.cash_flow"
        );
        assert_eq!(
            serde_json::to_value(&self.change).unwrap(),
            wire["change"],
            "native field identity BybitTransactionRecord.change"
        );
        assert_eq!(
            serde_json::to_value(&self.cash_balance).unwrap(),
            wire["cashBalance"],
            "native field identity BybitTransactionRecord.cash_balance"
        );
        assert_eq!(
            serde_json::to_value(&self.fee_rate).unwrap(),
            wire["feeRate"],
            "native field identity BybitTransactionRecord.fee_rate"
        );
        assert_eq!(
            serde_json::to_value(&self.bonus_change).unwrap(),
            wire["bonusChange"],
            "native field identity BybitTransactionRecord.bonus_change"
        );
        assert_eq!(
            serde_json::to_value(&self.trade_id).unwrap(),
            wire["tradeId"],
            "native field identity BybitTransactionRecord.trade_id"
        );
        assert_eq!(
            serde_json::to_value(&self.order_id).unwrap(),
            wire["orderId"],
            "native field identity BybitTransactionRecord.order_id"
        );
        assert_eq!(
            serde_json::to_value(&self.order_link_id).unwrap(),
            wire["orderLinkId"],
            "native field identity BybitTransactionRecord.order_link_id"
        );
    }
}
impl NativeIdentity for BybitTransactionHistoryOptions {
    fn check_identity(&self, wire: &Value) {
        for key in wire.as_object().unwrap().keys() {
            assert!(
                [
                    "accountType",
                    "category",
                    "currency",
                    "baseCoin",
                    "type",
                    "startTime",
                    "endTime",
                    "limit",
                    "cursor"
                ]
                .contains(&key.as_str()),
                "unobserved native field BybitTransactionHistoryOptions.{key}"
            );
        }
        assert_eq!(
            serde_json::to_value(self.account_type).unwrap(),
            wire["accountType"],
            "native field identity BybitTransactionHistoryOptions.account_type"
        );
        assert_eq!(
            serde_json::to_value(self.category).unwrap(),
            wire["category"],
            "native field identity BybitTransactionHistoryOptions.category"
        );
        assert_eq!(
            serde_json::to_value(&self.currency).unwrap(),
            wire["currency"],
            "native field identity BybitTransactionHistoryOptions.currency"
        );
        assert_eq!(
            serde_json::to_value(&self.base_coin).unwrap(),
            wire["baseCoin"],
            "native field identity BybitTransactionHistoryOptions.base_coin"
        );
        assert_eq!(
            serde_json::to_value(self.r#type).unwrap(),
            wire["type"],
            "native field identity BybitTransactionHistoryOptions.r#type"
        );
        assert_eq!(
            self.start_time.map(f64::to_bits),
            wire["startTime"].as_f64().map(f64::to_bits),
            "native field identity BybitTransactionHistoryOptions.start_time"
        );
        assert_eq!(
            self.end_time.map(f64::to_bits),
            wire["endTime"].as_f64().map(f64::to_bits),
            "native field identity BybitTransactionHistoryOptions.end_time"
        );
        assert_eq!(
            self.limit.map(f64::to_bits),
            wire["limit"].as_f64().map(f64::to_bits),
            "native field identity BybitTransactionHistoryOptions.limit"
        );
        assert_eq!(
            serde_json::to_value(&self.cursor).unwrap(),
            wire["cursor"],
            "native field identity BybitTransactionHistoryOptions.cursor"
        );
    }
}
impl NativeIdentity for BybitRateLimitAccountRates {
    fn check_identity(&self, wire: &Value) {
        for key in wire.as_object().unwrap().keys() {
            assert!(
                ["classicAndUTA", "utaPro"].contains(&key.as_str()),
                "unobserved native field BybitRateLimitAccountRates.{key}"
            );
        }
        assert_eq!(
            self.classic_and_uta.to_bits(),
            wire["classicAndUTA"].as_f64().unwrap().to_bits(),
            "native field identity BybitRateLimitAccountRates.classic_and_uta"
        );
        assert_eq!(
            self.uta_pro.to_bits(),
            wire["utaPro"].as_f64().unwrap().to_bits(),
            "native field identity BybitRateLimitAccountRates.uta_pro"
        );
    }
}
impl NativeIdentity for BybitRateLimitRow {
    fn check_identity(&self, wire: &Value) {
        for key in wire.as_object().unwrap().keys() {
            assert!(
                ["level", "futures", "option", "spot"].contains(&key.as_str()),
                "unobserved native field BybitRateLimitRow.{key}"
            );
        }
        assert_eq!(
            serde_json::to_value(self.level).unwrap(),
            wire["level"],
            "native field identity BybitRateLimitRow.level"
        );
        self.futures.check_identity(&wire["futures"]);
        self.option.check_identity(&wire["option"]);
        self.spot.check_identity(&wire["spot"]);
    }
}
impl NativeIdentity for BybitTransactionType {
    fn check_identity(&self, _wire: &Value) {}
}
impl NativeIdentity for BybitVipLevel {
    fn check_identity(&self, _wire: &Value) {}
}
impl NativeIdentity for BybitProductType {
    fn check_identity(&self, _wire: &Value) {}
}
impl NativeIdentity for BybitAccountType {
    fn check_identity(&self, _wire: &Value) {}
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&input).unwrap();
    let output: Vec<_> = cases
        .into_iter()
        .map(|case| match case["type"].as_str().unwrap() {
            "BybitApiKeyInfo" => round_trip::<BybitApiKeyInfo>(case["value"].clone()),
            "BybitTransactionType" => round_trip::<BybitTransactionType>(case["value"].clone()),
            "BybitTransactionRecord" => round_trip::<BybitTransactionRecord>(case["value"].clone()),
            "BybitTransactionHistoryOptions" => {
                round_trip::<BybitTransactionHistoryOptions>(case["value"].clone())
            }
            "BybitRateLimitRow" => round_trip::<BybitRateLimitRow>(case["value"].clone()),
            "BybitVipLevel" => round_trip::<BybitVipLevel>(case["value"].clone()),
            "BybitProductType" => round_trip::<BybitProductType>(case["value"].clone()),
            "BybitAccountType" => round_trip::<BybitAccountType>(case["value"].clone()),
            _ => panic!("unknown shape"),
        })
        .collect();
    println!("{}", json!(output));
}
