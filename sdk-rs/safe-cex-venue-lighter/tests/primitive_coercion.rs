//! Out-of-type owner fields read by host-style bodies take the JS result
//! (ToNumber, or a TypeError) instead of panicking the host.
use safe_cex_venue_lighter::{
    exchange_error_recovery::{
        ErrorRecoveryCall, ExchangeErrorRecoveryHost, should_short_circuit_signer_retry,
    },
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_signing::{HostThrownValue, SigningResult, TransactionSigningError},
};
use std::rc::Rc;

struct Returns(AccountValue);
impl ErrorRecoveryCall for Returns {
    fn call(&self, _: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(self.0.clone())
    }
}
struct Plain;
impl AccountObject for Plain {
    fn get(&self, _: &str) -> SigningResult<AccountValue> {
        Ok(AccountValue::Undefined)
    }
}
/// Clock reads 1000; `signerRetryBlockedUntil` holds the value under test.
struct Host(AccountValue, AccountValue);
impl ExchangeErrorRecoveryHost for Host {
    fn owner(&self) -> AccountValue {
        AccountValue::Null
    }
    fn get(&self, _: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        Ok(match key {
            "signerRetryBlockedUntil" => self.0.clone(),
            _ => AccountValue::Undefined,
        })
    }
    fn set(&self, _: &AccountValue, _: &str, _: AccountValue) -> SigningResult<()> {
        unimplemented!()
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>> {
        assert_eq!(name, "virtualClock.getCurrentTimeMs");
        Ok(Rc::new(Returns(self.1.clone())))
    }
    fn member(&self, _: &AccountValue, key: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>> {
        assert_eq!(key, "isWasmRuntimeIssueMessage");
        Ok(Rc::new(Returns(AccountValue::Boolean(false))))
    }
    fn instance_of_error(&self, _: &AccountValue) -> SigningResult<bool> {
        unimplemented!()
    }
    fn timeout_regex_test(&self, _: &AccountValue) -> SigningResult<bool> {
        unimplemented!()
    }
    fn template_string(&self, _: AccountValue) -> SigningResult<Vec<u16>> {
        unimplemented!()
    }
}
fn s(text: &str) -> AccountValue {
    AccountValue::String(text.encode_utf16().collect())
}
fn type_error(message: &str) -> SigningResult<bool> {
    Err(TransactionSigningError::Generated(TransactionBuildError {
        name: "TypeError",
        message: message.encode_utf16().collect(),
    }))
}

#[test]
fn non_number_owner_field_takes_js_comparison_result() {
    // `virtualClock.getCurrentTimeMs() < this.signerRetryBlockedUntil` with now = 1000.
    let cases: Vec<(AccountValue, SigningResult<bool>)> = vec![
        (AccountValue::Number(5000.0), Ok(true)),
        (s(" 5000 "), Ok(true)),
        (s("0x1f40"), Ok(true)),
        (s("abc"), Ok(false)),
        (s(""), Ok(false)),
        (AccountValue::Undefined, Ok(false)),
        (AccountValue::Null, Ok(false)),
        (AccountValue::Boolean(true), Ok(false)),
        (
            AccountValue::Symbol(HostThrownValue::new(())),
            type_error("Cannot convert a Symbol value to a number"),
        ),
        (
            AccountValue::Object(Rc::new(Plain)),
            type_error("Cannot convert object to primitive value"),
        ),
    ];
    assert_eq!(cases.len(), 10);
    for (index, (field, expected)) in cases.into_iter().enumerate() {
        let actual = should_short_circuit_signer_retry(&Host(field, AccountValue::Number(1000.0)));
        assert_eq!(actual, expected, "case {index}");
    }
}

#[test]
fn retry_deadline_comparison_keeps_bigint_precision_and_lexical_strings() {
    let cases = [
        (
            AccountValue::BigInt("9007199254740993".parse().unwrap()),
            AccountValue::Number(9007199254740992.0),
            true,
        ),
        (
            AccountValue::BigInt(5000.into()),
            AccountValue::Number(1000.0),
            true,
        ),
        (s("10"), s("2"), false),
        (s("2"), s("10"), true),
    ];
    for (deadline, clock, expected) in cases {
        assert_eq!(
            should_short_circuit_signer_retry(&Host(deadline, clock)),
            Ok(expected)
        );
    }
}
