# Paradex hosted Stark consumer (mode 2)

Status: PLAN ONLY. No source, Cargo feature, export, host change, activation or
deployment is authorized by this document. #2126 (live scalar exclusion) stays
HELD until the working route below exists, including today's batch path.
Owner: claude-m26ps4. Reviewers: codex-9czdjh (shared envelope, Paradex #8169
decisions D-a/D-b/D-c), claude-kuhwxi (security, required). Landing follows the
exact-tree GO from claude-8x5tm1.

Basis: [RUST_HOSTED_SIGNING_ENVELOPE.md](RUST_HOSTED_SIGNING_ENVELOPE.md) (#2156,
"Second declared consumer: Paradex Stark", amended here for batch), engine
sign-meta (#2163), tealstreet-next PARADEX_HOSTED_STARK_AND_SUBKEYS.md (#8169)
and its device originals ea795490356. Mode 3 (server-held subkeys) is out of
scope and still waits for Sam's explicit infrastructure go.

## Why

Mode 2 replaces today's live uploaded-key signing for hosted Paradex accounts:
the device keeps the Stark scalar and signs, and the server never receives it.
It must therefore keep every operation the live path performs today, including
native batches of 1..=10 plain LIMIT orders (overseer ruling). It must not
reintroduce the legacy digest-only `payload_request(hash)` route: the device
signs only what it can rebuild from a closed descriptor.

## Order of work

1. SDK: the closed descriptor, the one native projection and digest builder,
   peer validation and a pure WASM export, behind a non-default feature.
2. SDK: the Paradex origin moves its delegated path from `payload_request(hash)`
   to managed `Action::Sign` with StarkDigest operations and the descriptor
   (sign-meta gives owner/slot/role). The legacy route and Rust's delegated
   `orders.len() > 1` refusal (trading.rs, "payloadSigner ... signs one order
   per request") are removed in the same cutover. That deliberately changes
   capability: in Rust the delegated route IS the mode-2 device route, and no
   Rust payloadSigner path remains to keep the old limit. The batch capability
   exists only when the peer negotiates `paradex.orderBatch/1`. The legacy
   TypeScript `payloadSigner` in tealstreet-next safe-cex is untouched and keeps
   its one-order limit. No dual route.
3. tealstreet-next device consumer, after Copybara imports step 1 and the shared
   `@tealstreet/safe-cex-wasm/hosted-signing` wire module (codex-9czdjh) lands.
   `buildInitPayload` key exclusion and the public-subkey descriptor ship in this
   step together with the working route, never earlier and never as a
   `publicOnly` flip.

No NX-local typed shim, no TS Stark math and no second wire spelling.

## Frame

Exactly #2156: `sign-req.d = {id, type:'op', v:2, exchangeName:'paradex',
accountName, request:{attemptId, deadlineMs, receipt, slot, role, binding,
descriptor}}`; `sign-res.d = {id, type:'op', v:2, attemptId, result}` or the
closed error arm. Never legacy `config`/`wsAuth`. `attemptId` is a JSON number
that must be a nonnegative safe integer; the receipt nonce stays a canonical
decimal string.

## Descriptor

One family-tagged closed object at `request.descriptor`. Every object is closed
(unknown keys refuse). Keys are the literal spellings below; each Rust variant
struct carries its own `#[serde(rename_all = "camelCase", deny_unknown_fields)]`,
so nothing relies on enum-level renaming. Integers are JSON numbers that must be
nonnegative safe integers (at most 2^53-1), refused rather than rounded.
Decimals are the canonical native `num::format` strings. Each optional key is
REQUIRED-PRESENT, with a value or `null`; null and absence are not
interchangeable.

```text
{ "family":"paradex", "op":"auth", "revision":"paradex/1",
  "timestampS": int, "expirationS": int }
{ "family":"paradex", "op":"orderCreate", "revision":"paradex/1",
  "target": "orders"|"ordersBatchSingle", "order": Order }
{ "family":"paradex", "op":"orderBatch", "revision":"paradex/1",
  "items": [Order, ...] }                                   // 1..=10
{ "family":"paradex", "op":"orderModify", "revision":"paradex/1",
  "id": string, "timestampMs": int, "market": string, "side": Side,
  "orderType": OrderType, "size": decimal, "price": decimal,
  "triggerPrice": decimal|null }

Order = { "timestampMs": int, "market": string, "side": Side,
  "orderType": OrderType, "size": decimal, "price": decimal|null,
  "triggerPrice": decimal|null, "instruction": Instruction,
  "flags": ["REDUCE_ONLY"]|null, "stp": "EXPIRE_TAKER"|null,
  "clientId": string|null }
Side = "BUY"|"SELL"
OrderType = "LIMIT"|"MARKET"|"STOP_LIMIT"|"STOP_MARKET"|"TAKE_PROFIT_MARKET"
Instruction = "GTC"|"IOC"|"POST_ONLY"
```

Negotiated recipe ids map one to one and closed: `paradex.auth/1`,
`paradex.orderCreate/1`, `paradex.orderBatch/1`, `paradex.orderModify/1`. Batch
is advertised separately. A peer without `paradex.orderBatch/1` makes the origin
refuse a batch Unsupported/NotSent before queueing; it is never split into
single orders.

## Presence table (derived from native `prepare`, `native_batch` and modify)

Native trading.rs is the authority. These rows transcribe its current behavior.
They are pinned by originals that compare against native fixtures, not newly
asserted rules.

| orderType | price | triggerPrice | flags | Native source |
| --- | --- | --- | --- | --- |
| LIMIT | value | caller's trigger_price, value or null | REDUCE_ONLY iff reduce_only, else null | kind LIMIT (including a MARKET with an execution price, which becomes IOC LIMIT) |
| MARKET | null | caller's trigger_price, value or null | REDUCE_ONLY iff reduce_only, else null | kind MARKET |
| STOP_LIMIT | value | value (required) | null (reduce-only refused natively) | Conditional |
| STOP_MARKET | null | value (the order's price) | ["REDUCE_ONLY"] | StopLoss |
| TAKE_PROFIT_MARKET | null | value (the order's price) | ["REDUCE_ONLY"] | TakeProfit |

- instruction: "IOC" when an execution price converted the order; otherwise
  IOC for IOC/FOK, POST_ONLY for PostOnly, and GTC otherwise.
- stp and clientId: both non-null for a top-level order, both null for an
  attached child. clientId is 1..=64 chars with no CR/LF, unique within a
  command (native `validate_caller_ids`).
- Batch item: exactly native `native_batch` eligibility. That means orderType
  LIMIT, triggerPrice null, no attached TP/SL legs, not close, and a top-level
  order (stp and clientId non-null, unique across items). Reduce-only is
  allowed: flags is `["REDUCE_ONLY"]` or null. On the wire, instruction is
  GTC|IOC|POST_ONLY. Native excludes a FOK time-in-force at the origin, but FOK
  and IOC both emit "IOC", so the peer validates the wire value and the origin
  applies `native_batch` itself.
- Modify: id is non-empty and renders the one encoded path component; price is
  always present (native signs `0` for MARKET); triggerPrice follows the
  native modify builder.

## What is hashed and what is authorized

Hash only what native hashes, through the one native builder:
- auth: POST, `/v1/auth`, body 0, timestampS, expirationS;
- order: timestampMs, market, side, orderType, quantum(size), and quantum(price) or 0;
- modify: the same plus id.

The unhashed fields are still part of the authorized operation.
- Origin: ONE projection function maps the frozen native Placement/Modify to the
  descriptor. Before sending the frame, the origin asserts that the descriptor
  equals the projection of the exact body and target it will publish,
  including the encoded modify id. Unsigned economic fields can't drift around
  the same digest.
- Peer: validates every closed value and the presence table above, and binds
  account, chain and expected public subkey from its OWN installed slot. Only
  then does it hash. It signs nothing it can't fully describe.

## Pure builder and export

```rust
pub fn digests(d: &ParadexDescriptor, b: &ParadexBinding) -> Result<Digests, DescriptorError>;
pub struct Digests(/* 1..=10 */ Vec<[u8; 32]>);  // auth/create/modify: exactly 1; batch: N, item order
pub fn validate(d: &ParadexDescriptor, b: &ParadexBinding, peer_now_ms: u64, deadline_ms: f64)
    -> Result<(), DescriptorError>;
```

Local signing (`Credentials::auth/order/modify`) is refactored to call the same
`digests` before its local Stark sign. Local and hosted therefore hash the same
bytes, and existing sign vectors stay byte-exact. `ParadexBinding {account,
chain_id}` is built only from trusted config/setup. `DescriptorError` is closed
and sanitized.

Pure WASM export, linked only into the Paradex/device host build:

```text
paradexHostedDigests(descriptorJson: string, bindingJson: string,
                     peerNowMs: number, deadlineMs: number)
  -> { "digests": ["0x<64 hex>", ...] }   // length 1, or N for orderBatch, in item order
   | { "error": "<closed code>" }
```

The export reads no clock and holds no key. The device passes its own clock read
at the actual signing moment. After any async work it calls again, rechecking
the deadline and clock revision immediately before signing; validation is never
cached from receipt time.

## Signing and result

One request, one receipt, one task. The peer builds ONE SignRequest with N
StarkDigest operations (N=1 except batch; N≤10 is inside the 32-operation
limit) on its own installed ref. It returns exactly N 64-byte values in item
order. Each r/s is range-checked and verified against its own recomputed digest
and the same bound public subkey. The result is all-or-nothing: if the deadline
passes, a cancel arrives or a signature fails during partial signing, there is
no partial result. The origin checks count, width and order, verifies each
value against its digest and the bound subkey, and only then renders the native
decimal felt pair into the native body.

## Time policy

These are the #2156 security pins, applied at the actual signing moment:
- the original deadline is checked first, and equal or later refuses;
- auth requires expirationS == timestampS + 300 exactly, with timestampS within
  peer_now ±30 s;
- order, batch-item and modify timestampMs must be within peer_now ±30,000 ms,
  inclusive.

## Originals before source (SDK)

Positives:
- auth;
- each orderType through orderCreate, both targets;
- a reduce-only LIMIT, single and in a batch;
- batches of 1 and 10 items;
- modify.

Each positive is checked against native fixtures with WHOLE-operation native
body equality: the local path and the hosted path produce byte-equal bodies,
signatures included. Digest equality alone is not enough. An independent Stark
verifier checks each signature.

Refusals, all with zero callbacks and no HTTP:
- descriptor shape:
  - every unknown or missing key, and every null/absent swap;
  - an unsafe, negative, fractional or string integer;
  - an exponent or `+` decimal;
  - a presence-table violation;
- batch:
  - a 0- or 11-item batch;
  - duplicate client ids;
  - a bad middle item;
  - a non-eligible item;
  - an un-negotiated batch;
- operations and targets:
  - each excluded op (cancel-by-signature, onboarding, transfer, withdraw, unknown);
  - a method, path or subkey substitution;
  - a descriptor carrying a digest;
  - an origin projection mismatch;
- signatures:
  - swapped, missing, extra or invalid signatures;
- timing:
  - each time-policy edge (inclusive bounds);
  - a deadline or cancel during partial signing, with no partial result.

## Gates

The feature is off by default. Non-adopters show zero byte growth (paired
Bybit, Bitget and Kraken). One local composed combined measurement runs on the
current tip, plus Paradex when it is adopted. Scoped tests, strict checks, bans
and ratchets apply, followed by one frozen-head CI run, READY and an exact-tree
GO. The NX device consumer has its own PR to tealstreet-next master, which the
overseer merges after GO, with no deployment.
