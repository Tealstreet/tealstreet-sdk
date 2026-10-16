# Polymarket constants and authentication records — gate evidence

Historical evidence for the original slice committed as
`6471f50a7cdebfbc94544d1c805a052f01ae1c41`. These results are not a
gate claim for later branch heads.

Integration base: `a3d514bec82d6760d4e9e7a27186f992d175c36d`, merged in
`9125e9f557873d19690b1f92a7e71420d5773b49`. Remote integration was rechecked
before the final scoped gates. Source venue read: all seven TypeScript files,
2,994 lines. No TypeScript runtime source remains modified.

Python 3.14 `tomllib`, seven venue manifests: initial **0/0/141**, now
**13/4/124** (ported/divergent/pending), total 141, not-applicable 0.
The four divergent authentication records cite the canonical CHOSEN UTF-8
boundary; these are representation restrictions, not runtime implementations.

## Original slice scoped gates

Commands are in CLAUDE.md. Each command exited 0; Cargo used `--jobs 1`
and no Cargo invocations overlapped. No Yarn shim or package script was used.

- Package formatting check.
- Package test suite: 4 tests passed, 0 failed (3 auth, 1 constants).
- Host clippy, all targets, `-D warnings`.
- wasm32-unknown-unknown clippy, library, `-D warnings` (not wasm runtime execution).
- Real parity indexer through `tests/scoped_parity.py`: 141 rows,
  24 public Rust items, no exemptions; only this venue was indexed.
- Missing SAFE_CEX_TYPESCRIPT_PATH: oracle refused with the named assertion
  `SAFE_CEX_TYPESCRIPT_PATH is required`.

Host: Node v26.7.0, rustc 1.99.0 (b940084d7 2026-09-28).

## Witness scope

`constants_actual_typescript` executes the actual venue constants module and
actual shared enum dependencies in one Node process. Every one of 13 rows has
an individual production mutation below. Full own-key sets/order, table values,
EIP-712 array order, numeric enum forward/reverse declarations, and all 15 native
numeric paths are compared. Tables are ordered declaration data; runtime
prototype lookup consumers remain pending and must use shared js_table_lookup.

The auth corpus executes actual deriveApiKey and buildL2Headers with recording
HTTP and real ethers HMAC. It checks 15 declared fields, 36 numeric ingress
decodes (72 native-bit comparisons), and 32 Unicode boundary cases (20
acceptances, 12 lone-surrogate refusals). Type interfaces are erased at runtime;
compiler schema checks are separate from these executable source consumers.
One cached Node capture serves all three tests, including failed captures.
The control runner asserts one capture per execution; the previous test harness
made three captures. Controls run one scoped test target per mutation/restoration,
never one subprocess per fixture case. No shared corpus was edited or regenerated.

No Rust REST methods, signing functions, exchange lifecycle, store updates,
order writes or socket behavior are claimed. Remaining: 124 manifest rows,
including 23 types. No live credentialed test or production deployment occurred.

## Auth mutation transcript

```text
credentials symmetric swap: FAIL native_credentials_api_key
credentials symmetric swap: RESTORED PASS (3 tests)
response symmetric swap: FAIL native_response_api_key
response symmetric swap: RESTORED PASS (3 tests)
headers symmetric swap: FAIL native_headers_address
headers symmetric swap: RESTORED PASS (3 tests)
numeric sign loss: FAIL numeric ingress changed Number bits at $.nonce
numeric sign loss: RESTORED PASS (3 tests)
oracle sign loss: FAIL nonce_bits_before_json
oracle sign loss: RESTORED PASS (3 tests)
optional null collapse: FAIL present_null:PolymarketL2Headers.POLY_PASSPHRASE
optional null collapse: RESTORED PASS (3 tests)
heritage blindness: FAIL inherited_required_inventory
heritage blindness: RESTORED PASS (3 tests)
actual source inherited field: FAIL declared field inheritedCredential
actual source inherited field: RESTORED PASS (3 tests)
unresolved heritage: FAIL unresolved_heritage_refusal
unresolved heritage: RESTORED PASS (3 tests)
surrogate replacement at ingress: FAIL string_boundary_refusal:PolymarketCredentials
surrogate replacement at ingress: RESTORED PASS (3 tests)
10 mutations rejected at named assertions; 10 restorations passed
```

## Constants mutation transcript

```text
POLYMARKET_CONFIG: FAIL constant:POLYMARKET_CONFIG:
POLYMARKET_CONFIG: RESTORED PASS
ENDPOINTS: FAIL constant:ENDPOINTS:
ENDPOINTS: RESTORED PASS
ORDER_TYPE_MAP: FAIL constant:ORDER_TYPE_MAP:
ORDER_TYPE_MAP: RESTORED PASS
ORDER_SIDE_MAP: FAIL constant:ORDER_SIDE_MAP:
ORDER_SIDE_MAP: RESTORED PASS
ORDER_SIDE_REVERSE: FAIL constant:ORDER_SIDE_REVERSE:
ORDER_SIDE_REVERSE: RESTORED PASS
ORDER_STATUS_MAP: FAIL constant:ORDER_STATUS_MAP:
ORDER_STATUS_MAP: RESTORED PASS
SignatureType: FAIL constant:SignatureType:
SignatureType: RESTORED PASS
EIP712_DOMAIN: FAIL constant:EIP712_DOMAIN:
EIP712_DOMAIN: RESTORED PASS
EIP712_ORDER_TYPES: FAIL constant:EIP712_ORDER_TYPES:
EIP712_ORDER_TYPES: RESTORED PASS
WS_MESSAGE_TYPES: FAIL constant:WS_MESSAGE_TYPES:
WS_MESSAGE_TYPES: RESTORED PASS
WS_CONFIG: FAIL constant:WS_CONFIG:
WS_CONFIG: RESTORED PASS
RATE_LIMITS: FAIL constant:RATE_LIMITS:
RATE_LIMITS: RESTORED PASS
TIMEFRAME_MAP: FAIL constant:TIMEFRAME_MAP:
TIMEFRAME_MAP: RESTORED PASS
13 production mutations rejected; 13 restorations passed
```

## Integration refresh procedure

Integration `5b3f63f2ea` was merged on the separate refresh branch, preserving
the accepted original branch head. The control runners now require exit 101
and can retain every complete mutation/restoration output using
`POLYMARKET_CONTROL_LOG_DIR`. Refreshed results must name their exact tested SHA
and be reviewed before the refresh branch is pushed; the historical sheet above
is not reused as evidence for it.

Read `packages/prediction-core/CLAUDE.md` and all production source before any
market or position shape work. Events, outcomes, holdings, bounded binary payoff,
probability normalization, neg-risk exposure grouping and resolution risk belong
to that shared model. This slice maps constants and authentication records only.
