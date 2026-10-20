# ApeX Omni native signing plan and evidence

The runtime uses `safe-cex-crypto::zklink` and native request serializers. The
installed `zklink-web-sdk-alpha@1.0.2` is an independent test oracle only; no
JavaScript SDK, provider adapter, or embedded JavaScript executes in Rust.

## Inputs and key derivation

API credentials (`key`, `secret`, `passphrase`) and `omniKeySeed` are independent.
The Omni seed is the optional-`0x`, exactly 65-byte packed Ethereum signature
accepted by TS `initZklinkSigner(seed)`. Decode its hex bytes verbatim, including
the final recovery byte. Do not hash its text, decode a private key, ask a wallet
to sign, or sign a new EIP-191 message. The existing native primitive derives
`effective = SHA256(seed)`, repeatedly SHA256s that value until its big-endian
integer is a valid Alt Baby Jubjub scalar, then uses Rescue/MuSig. It already has
independent npm and pinned-native SDK test corpora. Reject malformed seed before
any authenticated order write; never log credentials or seed bytes.

## Order preparation and exact contract bytes

Read `contractConfig.perpetualContract[].l2PairId` and account `id`, maker and
taker fee rates from successful configuration/account reads. Resolve caller IDs
before signing: the ID is sent verbatim as both `clientId` and `clientOrderId`
and controls the signed nonce. Nonempty, whitespace-free caller IDs are accepted
only for a single regular native order; split or attached/conditional shapes
reject before writes. Generated IDs use the active TS exchange format
`apexomni-{accountId}-{milliseconds}-{six decimal digits}`, not the unused utils
helper's different prefix. Each bracket leg has its own generated ID/signature.

Adjust amount and prices using TS's nearest-step rounding (`num::adjust`). A
market order uses adjusted mark times 1.02 for buy or 0.98 for sell. TP/SL
execution prices use adjusted trigger times 1.005 for buy or 0.995 for sell.
Attached children reverse parent direction and sign the exact parent lot size.
Reject split brackets before signing/writing because no atomic multi-parent
contract exists. Preserve TS regular splitting, with each lot signed separately.

For each parent/leg, SHA256 its UTF-8 client ID as an unsigned big-endian integer
`H`. Compute nonce `H mod (2^32-1)` and slot
`Number((H mod (2^64-1))/(2^32-1))`. The SDK's WASM argument truncates/wraps slot
to u32. Compute account `id mod (2^32-1)`, subaccount zero, maker/taker rates
`ceil(parseFloat(rate)*10000)`; wrapper uses absolute maker rate and the exchange
explicitly sets subsidy false, including negative maker rates. Native code checks
the documented fee range before encoding. Scale decimal price and size by 10^18,
truncating toward zero using decimal text, not binary multiplication.

The official pinned SDK serializes **36 bytes** in this order:

| Field | Encoding |
| --- | --- |
| message type | one byte `fe` |
| account | u32 big-endian |
| subaccount | one byte zero |
| slot | low 16 bits of the u32 builder argument, big-endian |
| nonce | low 24 bits of the u32 builder argument, big-endian |
| pair | low 8 bits of the u16 builder argument |
| direction | one byte, buy 1 / sell 0 |
| scaled size | 40-bit decimal float: 35-bit mantissa then 5-bit exponent |
| scaled price | 120-bit unsigned big-endian |
| maker, taker | one byte each |
| subsidy | one byte zero |

Size packing follows the SDK's closest representable value at or below the
scaled amount: choose minimal decimal exponent fitting the 35-bit mantissa,
compare that candidate with the previous exponent's maximum mantissa, and use
the closer one (previous exponent wins a tie). The packed word is
`mantissa << 5 | exponent`, in five big-endian bytes. A naive decimal truncation
alone fails immediately above the maximum mantissa. Check numeric boundaries
before calling the crypto primitive; no assertions or panics for user input.

The crypto primitive consumes those bytes MSB-first, zero-pads to 736 bits,
multipacks into BN256 field elements, Rescue-hashes, writes the first output as
32 little-endian bytes, derives its deterministic signing seed, and emits the
packed R point plus little-endian S. Send the 64-byte signature as lowercase hex
without `0x`; the public key is not a REST order field. Expiration and limit fee
are **not** in the L2 signed bytes. Expiration is
`(ceil((nowMs + 30 days)/1 hour) + 168) * 1 hour`, in milliseconds.

Limit fee follows the two numeric boundaries in TS: decimal
`price * takerFeeRate * size`, convert to Number, interpret that Number's shortest
decimal in BigNumber, multiply by 10^6, convert to Number, ceil, divide by 10^6,
then six decimal places. Plain chained f64 multiplication changes `0.1 * 0.2 *
0.0005` from `0.000010` to `0.000011`. Native decimal multiplication preserves
the boundaries; tests also cover a product rounded upward before ceiling.

## REST API authentication and fallback

HMAC-SHA256 key bytes are **UTF-8 of standard padded Base64 of UTF-8 secret**.
Do not base64-decode the secret, or use the raw secret as HMAC key. Output is
standard padded Base64. Headers are `APEX-SIGNATURE`, `APEX-API-KEY`,
`APEX-PASSPHRASE`, `APEX-TIMESTAMP`.

Primary timestamp is integer milliseconds. GET/DELETE canonical message is
`timestamp + METHOD + /api + path + (? + qs if nonempty)`. Other methods append
canonical serialized data instead of the query string. The serializer sorts
known flat ASCII camelCase keys, percent-encodes RFC3986 UTF-8 with uppercase hex,
preserves string `0`/`false` and empty values, and omits absent/null fields. Raw
string bodies are already serialized and must remain byte-for-byte unchanged.
Order preparation's outer legacy truthiness filter already omits false flags
before it produces the raw form body. Klines are the sole GET auth exception:
strip all headers. Public symbols still runs the signing path with public-mode
empty credentials, matching the active TS client.

Only an explicit response-envelope signing rejection (code 20016, code 401, or
case-insensitive message `sign`/`signature`) permits one legacy retry. It does
not authorize resending after ambiguous transport failure. The legacy timestamp
is floored to whole seconds but still expressed in milliseconds. Legacy qs
omits original falsy numeric/boolean values; typed requests must resolve those
before their string representation loses original type information. Its raw
body quirk is deliberate: `qs.stringify(string)` is empty, then `/v3/order`
appends the original raw body once. Preserve the pre-transform payload for retries:
the old TS code reused Axios-transformed response.config.data, causing cancellation
to sign an empty body and Node multipart leverage to sign random FormData internals
including its boundary and streams. Browser FormData produced yet another result.
This genuine TS bug is fixed alongside the port: both clients retain canonical
original payload fields across signing retries. Independent captures of the old
real createAPI adapter exposed the drift; native regressions pin stable corrected
cancel and leverage signatures with identical original transport payloads.
Other raw POST routes get no raw-body suffix.
No retry loop can issue a third signature attempt. Read-only network retries
remain separate and capped at three.

Leverage signs sorted qs fields but TS Axios transmits multipart/form-data. The
native signer produces a valid explicit multipart boundary/body; transport bytes
differ from signed canonical bytes by protocol design. Keep body and Content-Type
boundary synchronized.

## Private WebSocket authentication

Message to sign is `timestampMs + GET + /ws/accounts`, with the same Base64-text
HMAC key. Login is `{"op":"login","args":["<JSON string>"]}`; the inner object
contains `type`, `httpMethod`, `requestPath`, `apiKey`, `passphrase`, numeric
`timestamp`, and `signature`, in that order. The `args` element is a string, not
an object. On each socket generation send login, then subscribe immediately to
`ws_zk_accounts_v3`, `ws_user_v1`, `ws_notify_v1`, then ping. Do not wait for a
server login acknowledgement or append a `topics` field. Authentication signing
must complete before subscription; generation guards suppress stale sends.

## Test evidence and remaining phase checks

Cancel/replace resolves its source at FIFO activation and prepares signatures
before cancellation. Price-only amendments use remaining quantity; explicit
amount updates retain their replacement-quantity meaning. During cancellation,
bounded observations reconcile cumulative order totals with genuine executions
using source timestamps, then re-sign the remaining parent and bracket legs if
quantity changed. Full fills suppress replacement; failed cancellation never
places it. This uses observations through the cancel acknowledgement, rather
than claiming an atomic amendment or knowledge of later/unobserved fills.

`tests/sign.rs` pins independent HMAC values, exact raw legacy order bytes,
multipart canonical/body separation, nested login encoding, invalid credentials
and seed, fixed-18 decimal truncation, and fee rounding. Fresh signatures captured
from the installed npm 1.0.2 WASM cover a normal BTC parent, Unicode client ID,
negative maker fees with subsidy still false, pair narrowing, and mantissa-edge
rounding. Existing crypto corpora additionally mutate key, payload, nonce and
pin empty/maximum payloads. Runtime and tests must also pin generated parent/TP/SL
IDs and their independent signatures in a private tape, account/config failure
retention, one-shot signing rejection retry, ambiguous lost-write response, and
cancel/replace's fresh replacement ID.

Official byte/seed sources, pinned at commit
`3152525e2615ecbbdb56fd6728ce265ad2dde485`:

- [Contract serialization](https://github.com/zkLinkProtocol/zklink_sdk/blob/3152525e2615ecbbdb56fd6728ce265ad2dde485/types/src/tx_type/contract/contract_matching.rs)
- [WASM argument widths](https://github.com/zkLinkProtocol/zklink_sdk/blob/3152525e2615ecbbdb56fd6728ce265ad2dde485/bindings/wasm/src/tx_types/contract/contract_matching.rs)
- [Signature seed parsing](https://github.com/zkLinkProtocol/zklink_sdk/blob/3152525e2615ecbbdb56fd6728ce265ad2dde485/interface/src/json_rpc_signer.rs)
- [Size packing](https://github.com/zkLinkProtocol/zklink_sdk/blob/3152525e2615ecbbdb56fd6728ce265ad2dde485/types/src/basic_types/float_convert.rs)
