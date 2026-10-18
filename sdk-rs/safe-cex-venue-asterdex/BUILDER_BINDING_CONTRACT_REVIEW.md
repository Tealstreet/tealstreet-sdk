# Builder, approval and binding function contracts

This cohort covers exactly nineteen native/default compiler owners: four in
`builder`, nine in `builder_approval` and six in `binding::members`. It adds
seventeen contracts and refreshes the two builder order APIs to their current
`impl Into<PreparedOrderOpts>` compiler signatures. Their existing behavioral
cases remain intact. Other stored contracts retain their original bytes.

The selected runner obtains fresh private rustdoc, checks the exact owner/API
set and invokes the canonical accepted inventory and compiled native probes.
Eighteen owners execute production functions across 85 authored cases. The
zero-argument `no_wallet` constructor has one individual non-decoder review:
it constructs a fixed failure record without decoding or consulting a provider.
Its direct unit test checks both fields; the executed switch and approval
profiles also check the actual no-wallet call paths. No shared acceptance policy,
scanner guard or production workflow changes in this cohort.

| Owners | Native observations |
| --- | --- |
| `builder::present` | Concrete String decoding: present empty text versus null/boolean refusal. |
| `builder::{is_aster_maker_only_order,get_aster_builder_fee_rate_for_order}` | Existing maker/taker, post-only, order-type and fee-presence cases through the current PreparedOrderOpts input. |
| `builder::build_aster_approval_request` | Default registry without a proxy and explicit base URL; original target and headers. |
| `builder_approval::{result,check_result}` | Every result field, native approval status, Option presence and present empty text. |
| `builder_approval::{wallet_request,switch_to_aster_signature_chain}` | Actual wallet request/rejection forwarding, missing wallet, chain-add sequence and unsupported-provider response. |
| `builder_approval::{normalize_address,form_component}` | Address/checksum/zero-address decisions; actual form encoding of spaces, reserved and non-ASCII characters. |
| `builder_approval::{check_aster_builder_approved,approve_aster_builder_fee_with_browser_wallet}` | Actual approval workflows with injected wallet/fetch capabilities, ordered methods and visible failure outcomes. |
| `binding::members::optional` | Undefined absence, present empty String, null/boolean refusal and native NaN/negative-zero bits. |
| `binding::members::{path,invoke}` | Real assembled V1/V3 receiver dispatch plus native numeric balance-version state. |
| `binding::members::primitive_error` | Actual shared error subtype/name and empty/nonempty messages. |
| `binding::members::request` | Real assembled public client, factory and terminal adapter; object/array data and one GET. Numeric results are checked as native f64 bits before JSON projection. |
| `binding::members::listen` | Constructor-phase optional socket path returns a callable cleanup with undefined result. This bounded profile does not claim ready-socket delivery. |

The fixture modules supply terminal replies, scheduler and wallet capabilities;
they do not replace the workflow or binding functions under review. Aster assembly,
shared globals/client creation and actual private member bodies execute in the
binding profiles. The existing master runtime_fixture allocation is reused unchanged; existing
exchange fixture assertions remain intact.

The maintained references are `asterdex.builder-fee.ts` (maker/rate decisions),
`asterdex.builder-approval.ts` (approval result literals, address validation,
chain switching and approval workflows), and `asterdex.exchange.ts` (member
dispatch). The new source oracle executes the maintained approval module and
URLSearchParams against seventeen independent expectations in six roles. The
existing projection source oracle retains the maker/rate expectations.

Reproduce the bounded cohort and its compiled behavioral controls with the
provisioned TypeScript dependency and Rust toolchain:

```sh
node sdk-rs/safe-cex-venue-asterdex/scripts/accepted-builder-binding-contracts.mjs
python3 sdk-rs/safe-cex-venue-asterdex/scripts/accepted-builder-binding-controls.py
```

This is not a whole-package clearance: other owners, aliases, local declarations,
feature configurations, wasm and provider-specific transport routing retain their
separate obligations.

The baseline runner commit `cad590067a` executed the actual inventory before any
of this cohort's contract changes: all nineteen owners failed, seventeen for a
missing contract and two for their stale compiler API. After review, the same
selected inventory produced nineteen receipts with zero failed owners.

The permanent controls require compiled behavioral mismatches, reject compiler
errors as proof, restore the exact original source bytes in `finally`, and require
all nineteen receipts to pass after each restoration:

| Isolated production mutation | Required native failures |
| --- | --- |
| Optional typed decoding takes a JSON round trip | `binding::members::optional` loses native NaN. |
| The fixed no-wallet result reports success | Both actual chain-switch and browser-approval no-wallet profiles return the wrong success field. |
