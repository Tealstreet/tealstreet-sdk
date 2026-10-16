# Fourteen spot rows: measured gate receipt

Measured code SHA: `730dc7c5eab561fb3e84def9df4123f38710bdf5`.
Actual parent built on: `7fcfce7950c79407b19ec6f8987e37ec6eed9661`, the requested
integration head containing PR 7438. No unreviewed feature ancestor was added.
This receipt is an evidence-only child of that code commit. Gates below were
measured at the code SHA; they are not attributed to an untested later commit.
The final push follows integration merge-down and the mandated byte-comparison
of the crate's source/tests against this measured SHA; the delivery report states
the actual merge result and whether gate inputs changed.

## Movement and scope

Independent Python 3.12 `tomllib` parsing of the Hyperliquid manifest directory
compared each named row with `git show 7fcfce7950:<manifest>`: **14 pending->ported,
0 pending->divergent**. Census: **85/106/340 -> 99/106/326**, all 531 manifest rows
visited, stopping at the final row of the sorted directory. The only changed
manifest is `hyperliquidspot.types.ts.toml`:

- Eight native tables: BASE_URLS, WSS_URLS, ENDPOINTS, GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC, INFO_TYPES, EXCHANGE_TYPES, WEBSOCKET.
- Six closed aliases: Tif, Tpsl, LimitOrderType, TriggerOrderType, Grouping, OrderType.

Spot WSS mainnet uses `api.hyperliquid.xyz`, not the perpetual module's `api-ui`
host; spot INFO_TYPES contains 18 entries, not the perpetual module's 22. Both
own-key and array order are compared. Fixed table dimensions in the test are
2/2/2/1/1/18/14/2, independently declared before reading the source result.
The oracle checks the actual core/spot declaration ASTs for all six types and
their transitive references, then the actual strict TypeScript compiler checks
47 inputs (20 accepted, 27 rejected). These are a corpus floor, stopping at case
46 zero-based, not an exhaustive accepted-set proof. Only serde_json::from_str
is newly exercised here; source structural types and the existing strict core
numeric/string/map ingress are reused through real public spot aliases.

## Measured exits

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p
  safe-cex-venue-hyperliquid --all-targets --jobs 2`: **0**, finished successfully.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js
  parity:extract:check`: **0**; output read: 7,341 symbols / 512 files,
  `Would update 0 manifests; 0 added, 0 orphaned symbols.` The writer was run
  after manifest edits and reported `Updated 0 manifests` before the code commit.
- Mutation Cargo test below: **101**, all seven tests failed through the named
  native assertions, not through compilation. Mutation driver: **0** after
  checking the expected failures and restoration.
- Restored `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
  cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-hyperliquid
  --test spot_types --jobs 2 -- --nocapture`: **0**, **7 passed / 0 failed**.

Native assertions inspect decoded enum variants, record fields, optional
presence and binary64 price bits (including -0 and overflow infinities).
There are no encode/roundtrip assertions in this suite. The source script runs
one whole-corpus Node invocation per binary, not one subprocess per case.

## Discrimination proof

`python3 scripts/spot-types-control.py` records the committed SHA, modifies one
key/value in each of the eight production spot tables, and symmetrically swaps
three shared serde enum labels (Alo/Ioc, tp/sl, na/normalTpsl). Both production
files are restored in a finally block before the second Cargo test. The driver
must be run on the committed clean gate inputs; it records HEAD but does not
itself enforce clean git input equality. It was run after the feature commit.

All fourteen required named native failures were observed:
- `SPOT native table BASE_URLS`.
- `SPOT native table WSS_URLS`.
- `SPOT native table ENDPOINTS`.
- `SPOT native table GRAFANA_ENDPOINTS_EVERY`.
- `SPOT native table GRAFANA_ENDPOINTS_PERIODIC`.
- `SPOT native table INFO_TYPES`.
- `SPOT native table EXCHANGE_TYPES`.
- `SPOT native table WEBSOCKET`.
- `SPOT native Tif variant case 0`.
- `SPOT native Tpsl variant case 7`.
- `SPOT native Grouping variant case 13`.
- `SPOT native LimitOrderType TIF case 20`.
- `SPOT native TriggerOrderType TP/SL case 27`.
- `SPOT native OrderType limit TIF case 37`.

Concrete decoded value pairs: case 0 Tif expected Alo, decoded Ioc; case 7
Tpsl expected Tp, decoded Sl; case 13 Grouping expected Na, decoded NormalTpsl;
case 20 LimitOrderType expected Alo, decoded Ioc; case 27 TriggerOrderType
expected Tp, decoded Sl; case 37 OrderType limit expected Alo, decoded Ioc.
Each table's first own key/string gained `#CONTROL`, giving a distinct native
value pair. The table test catches each root panic so all eight roots execute.
No survivor occurred. Restoration equality was true for both production files;
the subsequent diff of source/tests against HEAD was empty.

Read-only sub-agent review found no correctness blocker, independently measured
14 moved rows and 47 source cases, and ran no Cargo or mutations. All 13 original
FINDING rows remain individually named in the committed disposition inventory.

Not run: full workspace compilation/tests, full crate tests, host clippy, wasm
clippy/build, network/live venue tests, or CI. Historical NULL/ZERO/STRINGS
cardinality remains **OPEN**, with residual targets retained. The old divergent
four-class inventory was refreshed for census only; no old row was promoted.
