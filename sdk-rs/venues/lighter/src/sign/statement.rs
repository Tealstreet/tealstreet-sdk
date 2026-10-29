//! The canonical statement a remote signer reads before it signs a Lighter
//! request: every signed field as plain JSON, which it re-derives into the
//! exact signed words instead of signing opaque bytes.
use super::{Account, Attributes, Fields, OrderInfo, TxInput, auth_message, encode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    Txs,
    Auth,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Tx {
    tx_type: u8,
    nonce: u64,
    expired_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<OrderInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    grouping_type: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    orders: Option<Vec<OrderInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attributes: Option<Attributes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    market_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    index: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    initial_margin_fraction: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    margin_mode: Option<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Statement {
    kind: Kind,
    chain_id: u32,
    account_index: u64,
    api_key_index: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    txs: Option<Vec<Tx>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deadline: Option<u64>,
}

impl Statement {
    fn new(account: &Account, kind: Kind) -> Self {
        Self {
            kind,
            chain_id: account.chain_id,
            account_index: account.account_index,
            api_key_index: account.api_key_index,
            txs: None,
            deadline: None,
        }
    }
    fn text(&self) -> String {
        serde_json::to_string(self).expect("finite statement fields")
    }
}

impl Tx {
    fn bare(tx_type: u8, nonce: u64, expired_at: u64) -> Self {
        Self {
            tx_type,
            nonce,
            expired_at,
            order: None,
            grouping_type: None,
            orders: None,
            attributes: None,
            market_index: None,
            index: None,
            initial_margin_fraction: None,
            margin_mode: None,
        }
    }
    fn of(input: &TxInput) -> Self {
        let mut tx = Self::bare(input.tx_type, input.nonce, input.expired_at);
        match &input.fields {
            Fields::Create(order, attributes) => {
                tx.order = Some(order.clone());
                tx.attributes = *attributes;
            }
            Fields::Grouped(grouping, orders, attributes) => {
                tx.grouping_type = Some(*grouping);
                tx.orders = Some(orders.clone());
                tx.attributes = *attributes;
            }
            Fields::Cancel {
                market_id,
                order_id,
            } => {
                tx.market_index = Some(*market_id);
                tx.index = Some(*order_id);
            }
            Fields::Leverage {
                market_id,
                fraction,
                margin_mode,
            } => {
                tx.market_index = Some(*market_id);
                tx.initial_margin_fraction = Some(*fraction);
                tx.margin_mode = Some(*margin_mode);
            }
        }
        tx
    }
    /// The fields for this transaction type, refusing any field it does not sign.
    fn fields(self) -> Result<Fields, String> {
        let refused = || "Lighter statement fields do not match the transaction type".to_owned();
        match self {
            Self {
                tx_type: 14,
                order: Some(order),
                attributes,
                grouping_type: None,
                orders: None,
                market_index: None,
                index: None,
                initial_margin_fraction: None,
                margin_mode: None,
                ..
            } => Ok(Fields::Create(order, attributes)),
            Self {
                tx_type: 28,
                grouping_type: Some(grouping),
                orders: Some(orders),
                attributes,
                order: None,
                market_index: None,
                index: None,
                initial_margin_fraction: None,
                margin_mode: None,
                ..
            } => Ok(Fields::Grouped(grouping, orders, attributes)),
            Self {
                tx_type: 15,
                market_index: Some(market_id),
                index: Some(order_id),
                order: None,
                grouping_type: None,
                orders: None,
                attributes: None,
                initial_margin_fraction: None,
                margin_mode: None,
                ..
            } => Ok(Fields::Cancel {
                market_id,
                order_id,
            }),
            Self {
                tx_type: 20,
                market_index: Some(market_id),
                initial_margin_fraction: Some(fraction),
                margin_mode: Some(margin_mode),
                order: None,
                grouping_type: None,
                orders: None,
                attributes: None,
                index: None,
                ..
            } => Ok(Fields::Leverage {
                market_id,
                fraction,
                margin_mode,
            }),
            _ => Err(refused()),
        }
    }
}

/// The statement for one `sendTx`/`sendTxBatch` request, in transaction order.
pub(super) fn txs(account: &Account, inputs: &[TxInput]) -> String {
    let mut statement = Statement::new(account, Kind::Txs);
    statement.txs = Some(inputs.iter().map(Tx::of).collect());
    statement.text()
}

/// The statement for one auth token.
pub(super) fn auth(account: &Account, deadline_seconds: u64) -> String {
    let mut statement = Statement::new(account, Kind::Auth);
    statement.deadline = Some(deadline_seconds);
    statement.text()
}

/// What a statement asks its signer to sign, re-derived through the same
/// encoder local issue uses.
#[derive(Debug, PartialEq)]
pub struct Derived {
    /// One signing preimage per signature, in order; each is signed as
    /// 32 fresh entropy bytes followed by these words.
    pub preimages: Vec<Vec<u8>>,
    /// The latest expiry the signatures authorize, in milliseconds.
    pub expiry_ms: u64,
}

/// Re-derives a statement for the signer that owns `own`. A statement in any
/// other spelling, for another chain, account or API key, or with a field its
/// transaction type does not sign is refused.
pub fn derive(text: &str, own: &Account) -> Result<Derived, String> {
    let refused = || "Lighter statement refused".to_owned();
    let statement: Statement = serde_json::from_str(text).map_err(|_| refused())?;
    // One spelling per request, so the signer reads exactly what it signs.
    if statement.text() != text
        || statement.chain_id != own.chain_id
        || statement.account_index != own.account_index
        || statement.api_key_index != own.api_key_index
    {
        return Err(refused());
    }
    match (statement.kind, statement.txs, statement.deadline) {
        (Kind::Txs, Some(txs), None) if (1..=super::MAX_BATCH).contains(&txs.len()) => {
            let mut preimages = Vec::with_capacity(txs.len());
            let mut expiry_ms = 0;
            for tx in txs {
                let (nonce, expired_at) = (tx.nonce, tx.expired_at);
                let input = encode(own, nonce, expired_at, tx.fields()?)?;
                expiry_ms = expiry_ms.max(expired_at);
                preimages.push(input.preimage);
            }
            Ok(Derived {
                preimages,
                expiry_ms,
            })
        }
        (Kind::Auth, None, Some(deadline)) => {
            let (_, data) = auth_message(own, deadline)?;
            Ok(Derived {
                preimages: vec![data],
                expiry_ms: deadline.checked_mul(1000).ok_or_else(refused)?,
            })
        }
        _ => Err(refused()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Builder, Order, Transaction, tx_input};
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    fn account() -> Account {
        Account::new(304, 42, 3).unwrap()
    }
    fn order(order_id: u64, order_type: u8) -> Order {
        Order {
            order_id,
            market_id: 1,
            is_buy: order_type == 0,
            price: 310_000,
            amount: if order_type == 0 { 1_000 } else { 0 },
            order_type,
            time_in_force: if order_type == 0 { 1 } else { 0 },
            reduce_only: order_type != 0,
            trigger_price: if order_type == 0 { 0 } else { 300_000 },
            order_expiry: None,
        }
    }
    fn every_kind() -> Vec<TxInput> {
        let builder = Some(Builder {
            account_index: 158_385,
            maker_fee: 0,
            taker_fee: 250,
        });
        [
            Transaction::Create {
                order: order(9, 0),
                builder,
            },
            Transaction::Grouped {
                grouping: 3,
                orders: vec![order(10, 0), order(11, 2), order(12, 4)],
                builder,
            },
            Transaction::Cancel {
                market_id: 1,
                order_id: 1_234_567_890_123,
            },
            Transaction::Leverage {
                market_id: 32_767,
                leverage: 20.,
                margin_mode: 1,
            },
        ]
        .iter()
        .enumerate()
        .map(|(nonce, tx)| tx_input(&account(), tx, nonce as u64 + 7, NOW).unwrap())
        .collect()
    }

    #[test]
    fn a_statement_re_derives_the_exact_words_local_issue_signs() {
        let inputs = every_kind();
        let text = txs(&account(), &inputs);
        let derived = derive(&text, &account()).unwrap();
        assert_eq!(
            derived.preimages,
            inputs
                .iter()
                .map(|input| input.preimage.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(derived.expiry_ms, NOW + super::super::TX_TTL);
        assert!(text.starts_with(r#"{"kind":"txs","chainId":304,"accountIndex":42,"apiKeyIndex":3,"txs":[{"txType":14,"nonce":7,"#));

        let token = auth(&account(), 1_700_000_060);
        assert_eq!(
            token,
            r#"{"kind":"auth","chainId":304,"accountIndex":42,"apiKeyIndex":3,"deadline":1700000060}"#
        );
        let derived = derive(&token, &account()).unwrap();
        assert_eq!(
            derived.preimages,
            vec![auth_message(&account(), 1_700_000_060).unwrap().1]
        );
        assert_eq!(derived.expiry_ms, 1_700_000_060_000);
    }

    #[test]
    fn another_signer_spelling_or_field_is_refused() {
        let text = txs(&account(), &every_kind());
        for other in [
            Account::new(300, 42, 3).unwrap(),
            Account::new(304, 43, 3).unwrap(),
            Account::new(304, 42, 4).unwrap(),
        ] {
            assert!(
                derive(&text, &other).is_err(),
                "another chain, account or key"
            );
        }
        let spaced = text.replacen("\"txs\":", "\"txs\": ", 1);
        assert!(derive(&spaced, &account()).is_err(), "a second spelling");
        let unknown = text.replacen("{\"kind\"", "{\"extra\":1,\"kind\"", 1);
        assert!(derive(&unknown, &account()).is_err(), "an unknown field");
        // A cancel that also carries an order field signs nothing it shows.
        let cancel = txs(&account(), &every_kind()[2..3]);
        let smuggled = cancel.replacen(
            "\"index\":1234567890123",
            "\"index\":1234567890123,\"marginMode\":1",
            1,
        );
        assert!(
            derive(&smuggled, &account()).is_err(),
            "a field the type does not sign"
        );
        let invalid = cancel.replacen("\"marketIndex\":1", "\"marketIndex\":255", 1);
        assert!(
            derive(&invalid, &account()).is_err(),
            "the encoder's own bounds"
        );
        let mixed = r#"{"kind":"auth","chainId":304,"accountIndex":42,"apiKeyIndex":3,"txs":[],"deadline":1}"#;
        assert!(derive(mixed, &account()).is_err(), "one kind per statement");
        assert!(derive(&cancel, &account()).is_ok(), "control");
    }
}
