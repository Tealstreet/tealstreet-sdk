# Core record contracts: Types and Crypto

This review covers 19 individually reviewed owners on the native/default compiler
configuration. It completes the dated census's eleven missing Types owners and
Crypto MessagePack parser, plus seven newer carrier/zkLink owners discovered by
the ordinary production inventory. It does not claim completion of the wider
core census or local/feature/wasm runtime coverage.

Each decoder contract calls its actual native API and compares independently
specified output fields, variants, numeric bits or signing results. Compiler
proposal reports supplied API identities only; their suggested classifications
and unexecuted test references were not accepted as behavioral evidence.

## Types

| Owner | Reviewed input and native evidence |
| --- | --- |
| OptionTerms | All eight named fields, exact right/style/settlement variants, expiry/strike bits |
| OptionGreeks | Four distinct Greek values/bits and units |
| MarketLimitsPrice | Named min field and its numeric bits |
| PositionOption | Optional Greek payload and every nested Greek field |
| OptionQuote | Symbol, seven required numbers, optional forward price and Greeks |
| OptionChainQuery | Underlying and independently optional expiry/right/strike bounds |
| AccountGreeks | Underlying, four distinct Greeks and units |
| OptionChainRow | Native market identity/base/quote/active/precision/limits and every quote field |
| OptionRight | Actual Call/Put identity; exact literal admission and tagged-object refusal |
| OptionTermsStyle | Actual European identity; exact literal admission and tagged-object refusal |
| OptionTermsSettlement | Actual Cash identity; exact literal admission and tagged-object refusal |
| CurrencyBalance | Declared native Balance versus AvailableLocked; as_balance and actual From<Balance> |
| ExchangeOptionsPolymarket | All five independently optional credential/signer/funder fields |

Floating field observations use IEEE754 bit strings so Node JSON formatting
cannot make native 1.0 differ from expected 1, or conceal signed-zero payloads.
The plain Serde DTO contract stays separate from Num's whole-entry numeric
pipeline. Account string fields keep their existing UTF-8 representation; this
change does not widen them to arbitrary UTF-16.

CurrencyBalance's hidden function-local derived Wire was replaced by a private
remote companion returning the actual CurrencyBalance. Its outer object guard,
untagged variant order and native numeric pipeline are retained. No extra
module-scope Deserialize owner or delegated-private-owner exemption was added.
The complete [9,6] alternate-row array is independently refused by the untagged
variant decoder even when the outer guard is removed; that is boundary evidence,
not a distinguishing guard-removal mutation. Native tests additionally inspect
both source shapes through raw-text and qualified decoder entrypoints, including
absent optional Balance metadata and the unnormalized REST alternate.

## Crypto

| Owner | Reviewed input and native evidence |
| --- | --- |
| msgpack::json_input | Actual retained JSON variants; nested numeric bits/order/duplicate semantics and four malformed-JSON refusals |
| zklink::contract::decimal | Raw unsigned decimal grammar, optional plus/underscore spelling, exact u128 endpoints and overflow errors |
| zklink::contract::closest_packable_amount | Actual decimal ingress and independently specified 35-bit mantissa boundary |
| zklink::contract::closest_packable_fee | Actual decimal ingress and independent 11-bit fee boundary |
| zklink::contract::Contract | All ten bounded post-WASM ABI fields and independently specified 36-byte signing preimages |
| zklink::contract::ContractSigner | Actual seed import, public-key/signature results against pinned npm SDK cases, and malformed-seed errors |

Contract's ordinary derived record decoder accepted the complete declaration-order
ABI array before repair. Its concrete Deserialize now uses the existing object
guard with a private remote companion. Integer/string/boolean/fixed-byte schemas
and the SDK signing algorithm are unchanged. Existing oracle artifacts remain
byte-identical. The raw-text/qualified regression asserts every field and the
actual signing preimage before rejecting the complete otherwise valid array.
The actual sign_contract method is also executed in both successful signer-import
witnesses; its typed Contract argument is not labeled a raw JSON decoder.

## Independent controls and validation

Nine existing Types record guards were each removed alone: AccountGreeks,
MarketLimitsPrice, OptionChainQuery, OptionChainRow, OptionGreeks, OptionQuote,
OptionTerms, PositionOption and ExchangeOptionsPolymarket. Each control compiled,
accepted the independently authored complete positional array, failed the named
raw native-output assertion plus trait/qualified shape assertions, then passed
after exact byte restoration. The repaired Crypto Contract guard underwent the
same independent compiled mutation/restoration. CurrencyBalance's separate
non-distinguishing control is described above; compilation errors and harness
assertions are not counted as mutation evidence.

Final native/default audit, package tests and host/wasm lint results are recorded
in the accompanying PR. The full audit is diagnostic: Crypto's Encoding and
WsOutputFormat retain their existing ledger debt. This change adds no exemptions,
widens no ledger entry and claims no whole-repository accepted-set success.
