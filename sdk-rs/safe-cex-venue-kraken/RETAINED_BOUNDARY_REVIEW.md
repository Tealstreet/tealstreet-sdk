# Retained response and closed String observers

Five inherited failures were reproduced on clean master `29bfd4e7ee` with
`--test accepted_helpers --test reads --test utilities --no-fail-fast`:
`error_constructor_preserves_literal_message_distinct_snapshots_and_presence`,
`read_chosen_boundaries`, `leverage_chosen_boundary`,
`numeric_capture_and_boundary`, and `chosen_boundary_live_witnesses`.
The helper/read/utility bodies and these targets were unchanged in the fresh
master used for this repair. These are observer migrations, not mapper changes.

## Error response

The live `createKrakenApiError` constructor retains its response object and -0.
`tests/error-retained-source.cjs` executes that unchanged declaration using the
maintained AST selector and separately observes JSON.stringify projection.
The compiled Rust helper also retains bits `8000000000000000` through
`retained_response_input`. Its explicitly computed `response_json_view` returns
bits `0000000000000000`, matching JSON.stringify. The migrated observer checks
both boundaries and retains all distinct field/presence/cause assertions.

## UTF-16 reach and the accepted String boundary

Raw parsed/native input retains D800. Shared numeric ingress projects closed
Rust Strings with `String::from_utf16_lossy`, yielding U+FFFD. The source keeps
D800; this final String projection is the existing shared representation choice,
not a claim that TypeScript changes the text. Depth refusal stays separate.

`accepted_boundary_observers` reaches the actual Scope of `KrakenExchange`:

- Spot private sockets call assembled `fetchWsToken`, whose signed POST client
  feeds `reads::fetch_ws_token`; the returned closed token is U+FFFD and the
  original native response still contains D800.
- Futures initialization calls assembled `fetchLeverageSettings`, whose signed
  GET feeds `reads::fetch_leverage_settings`; it clears OLD, sets the U+FFFD key
  to native 17.25 and reports no error. The original response key retains D800.
- `rest_account::fetch_historical_fills` calls the concrete `ReadHost::map_fills`.
  The constructed Scope witness supplies the complete source fill array and
  checks every native output field/bit, its unchanged D800 ID/input, then the
  actual invoke `to_typed::<Execution>` projection producing the U+FFFD ID.
  The prerequisite retained ingress fix is independently owned in #1146; its
  full historical-fill HTTP regression remains unchanged in this branch.
- Constructed `IncomeHost::account_log_page` invokes the assembled
  `_fetchAccountLogPage` and signed history client. Its four independent native
  income records match source categories, amounts, timestamps and fields after
  the explicit final String projection. Original raw booking ID units remain.

The twelve-case utility source census remains intact. Scalar helpers still
receive their actual decoded closed String arguments and compare native numeric,
optional and Boolean outputs. Complete fill/income rows compare every field
against the maintained live TS oracle, projecting only source `{units: [...]}`
captures into the declared closed String representation. No numeric capture is
projected. After #1146 the assembled fill host no longer calls the standalone
`fills.rs` DTO helpers: it maps native fields directly and retains D800 until the
Exchange trait's final `to_typed` projection. No production refusal is inferred
from those standalone helper tests; their closed String expectations describe
their public Rust API, while the native host witness checks source-exact units. An incomplete fill with only an ID is admitted then skipped for its
missing required symbol/time; it is not a decoder refusal. The source's two deep
ignored-field acceptance cases still produce explicit Rust depth refusals, and
string-valued typed numeric query arguments still refuse before any HTTP call.

## Verification

Run the three focused integration targets and `--lib accepted_boundary_observers`
with the explicit maintained TypeScript path. `scripts/retained-boundary-controls.py`
normalizes retained Error -0 and separately corrupts the final wire text. Each
fault must compile and fail its named native assertions; exact production-byte
restoration must pass. These controls do not count compiler failures as evidence.
The control leaves `helpers.rs` and `wire.rs` byte-identical to their originals.
Run full parity index, cancellation/override inventories, fmt check, native/Wasm
all-target Clippy, extractor check and the registered CI wiring test before READY.

This migration does not close the separate manifest full-block findings (for
example, comparing different unpaired asset/contract strings before a classifier).
It measures the twelve existing cases and the named five failures only; no
manifest status or audit finding is relabeled as parity by this repair.
