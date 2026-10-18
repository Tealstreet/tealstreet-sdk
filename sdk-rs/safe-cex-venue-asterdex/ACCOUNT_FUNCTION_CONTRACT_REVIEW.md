# Account and signing function contracts

This cohort classifies eleven native/default free functions from the remaining
Aster census. Six generic runtime-host helpers are reviewed separately. It adds
no production input API, alternate implementation or policy exemption. The
selected compiler baseline found all eleven owners and refused all eleven as
unclassified before the contracts were added.

`scripts/accepted-account-contracts.mjs` performs fresh private-item compiler
discovery, checks every exact function API, preserves the fixed per-owner case
counts, and executes all eleven real production functions. Every selected owner
must produce a native runtime receipt; classification-only receipts cannot pass.
Bootstrap rustdoc artifacts use `target/accepted-inventory`, separately from the
ordinary native cache. The native probe runner retains its separate sandbox and
`accepted-set-build` output. Cargo runs serially from the Rust workspace.

| Owners | Observations |
| --- | --- |
| `account_fetch::decode` | The three actual production instantiations: order, market and ticker vector. Complete returned fields, native numeric bits, optional and nested extension fields, empty ticker arrays, structural refusal and nonempty representation-error reasons. |
| `account_fetch::mapper_error` | Independently supplied native error name and message, including empty text, non-ASCII text and line breaks. |
| `types::present` | Every concrete type used by this adapter: String, bool, f64, Vec of String and STP mode. Actual Some presence, complete values, signed-zero bits, invalid scalar/sequence values and explicit null refusal. |
| `types::opaque_fields` | Raw JSON maps, complete nested extensions, negative zero, overflow, large integers, non-map refusal and a real native NaN visitor. |
| `account_helpers::handle_execution_report` | Existing/absent IDs, every status, fallback, inherited prototype identity, original native mapped/inherited variants, coercion refusal, filled and remaining bits. |
| `account_helpers::mirror_cumulative_order_fill` | Closed/open state, finite admission, clamped remaining, event-time precedence and actual clock callback count, early returns and conversion failures. |
| `account_helpers::parse_asset_balance_updates` | Both field-selection profiles, trimmed assets, complete rows, explicit/derived used, signed zero, overflow after validated operands and whole-batch refusal. |
| `account_helpers::handle_account_update` | Ordered balance/emit plans, empty arrays, invalid rows, and pointer equality of the emitted original account payload. |
| `account_helpers::get_account_update_upnl_deltas` | Side selection, previous PnL, missing/nonfinite input, ordered repeated accumulation, absent positions, own-key currency order and collateral fallback. |
| `api_helpers::generate_v1_signature` | Real shared HMAC signing, empty inputs, query ordering, distinct secrets and non-ASCII/NUL bytes. |
| `api_helpers::generate_aster_dex_code_message_signature` | Real shared EIP-712 signing, both chains, optional testnet, bare/prefixed keys, non-ASCII messages and malformed/zero-key refusals. |

The 128 fixed native observations include 68 expectations for seven helpers
captured from the maintained TypeScript bodies before any Rust execution.
`accepted-account-source.cjs` replays those unmodified bodies against the frozen
expectations. Its parent Store, clock and effect sinks are explicit dependencies;
they do not replace the helper under review. It loads the actual shared enums and
uses actual ethers/crypto. Fixed signature/numeric anchors and independent Python
HMAC checks were checked before freezing the corpus.

The other four functions are native Serde/error bridges rather than standalone
TypeScript runtime validators. Their observations execute the actual production
adapters. They neither invent a source validator nor claim TS rejects every
shape refused by the typed native surface. Wire-schema and numeric passthrough
admission retain their existing reviewed boundaries. The presence adapter is
instantiated at all five concrete types used in this file; this does not claim
arbitrary external implementations of Deserialize are equivalent.

Numeric observations read actual native binary64 bits before JSON projection;
NaN is a category rather than a platform-specific payload. Inherited status values
retain ProtoMember identity and arity rather than widening the shared OrderStatus.
The existing UTF-8, typed-output and snapshot boundaries remain declared in
[MANIFEST-SPEC](../parity/MANIFEST-SPEC.md).

`scripts/accepted-account-controls.py` corrupts all eleven real production bodies,
requires exactly eleven named native failures, then restores the exact original
bytes in `finally` and requires every owner to pass. Compiler failures cannot
satisfy this control. Logs, named failures and restoration hashes stay under the
worktree's `target/asterdex-account-function-controls` directory.

Run with the provisioned compiler dependency, from `sdk-rs`:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/absolute/path/to/typescript \
  node safe-cex-venue-asterdex/scripts/accepted-account-contracts.mjs
SAFE_CEX_TYPESCRIPT_PATH=/absolute/path/to/typescript \
  python3 safe-cex-venue-asterdex/scripts/accepted-account-controls.py
```

`--preflight` checks source expectations and fresh compiler classification only;
it does not execute native probes. This selected cohort does not claim whole-crate,
local-helper, alias, feature or WASM coverage.

The measured default/native replay passes all eleven owners and 128 observations,
with zero classification-only receipts, after all 68 actual-source expectations
pass. The grouped real-body faults compile and fail all eleven owners by name.
Exact restoration of the four production files returns every native owner to PASS;
no production change is retained.
