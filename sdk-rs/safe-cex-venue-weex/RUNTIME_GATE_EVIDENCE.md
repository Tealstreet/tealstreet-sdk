# WEEX API assembly and account lifecycle

Branch: `feat/rust-venue-weex-runtime`.
Actual authoring parent: `2455f13472068102e2d5192e7edc14fe32accf99` (integration); no separate unreviewed feature ancestor.
Native implementation/test gate SHA: `15a9ca8db900b63cb67c5f3908ed9378857c247a`.
Manifest/control preparation SHA: `007391dddaa4569e59046fad35df5c59292524f0`.

## Rows moved

Independent committed-manifest `python3.12`/`tomllib` census against the authoring parent: **8 pending -> ported, 1 pending -> divergent**. WEEX has 156 rows, moving from 113 ported / 31 divergent / 12 pending to 121 ported / 32 divergent / **3 pending**.

The eight ports are `serializeWeexParams`, `createAPI`, `WeexExchange.constructor`, `WeexExchange.validateAccount`, `WeexExchange.getAccount`, `WeexExchange.start`, `WeexExchange.tick`, and `WeexExchange.fetchIncomeHistory`. The divergent row is `WeexExchange.fetchIncomeHistoryRange`.

That range wrapper is **CHOSEN**: it calls the existing Bitget-owned `safe_cex_exchange::income_paging::paginate_income_range` with owned, typed `IncomeRecord` values, preserving the shared ledger instead of adding a parallel mutable JavaScript record graph. Its typed/string/live-alias and observer-drop boundaries follow the shared pager's own manifest. The wrapper retains 100-day / 100-row options, eager first request, 100 ms after every successful page including empty pages, no sleep after page rejection, and the source truncation warning. No venue pager or shared type/trait change was written.

Independent declaration lookup in committed Rust source found every target on all nine moved rows, including split runtime/callback carriers; zero missing targets. This is a declaration audit, not proof of arbitrary host implementations. The three class aggregates remain pending: `WeexExchange`, `WeexPublicWebsocket`, and `WeexPrivateWebsocket`.

## Measured baseline gates

At `15a9ca8db900b63cb67c5f3908ed9378857c247a`, both commands below completed with exit **0**:

```sh
/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test runtime
```

The test target reported **10 passed; 0 failed**, with an independent literal floor of **42 actual-source fixtures**. Last enumerated fixture: iteration 42, `range_invalid`. Nine named fixture tests each independently require one selected fixture; the complete native floor also covers all 42. Constructor own-field floor: **56**, with a separate ordered field-name receipt at the xhr-factory call and options identity assertion. Range-record floor: **1** for `range_short`, **0** for each other range fixture. Every native typed record field is asserted directly before constructing the generic value view; no decoded native output is re-encoded for comparison.

At `007391dddaa4569e59046fad35df5c59292524f0`, both commands below completed with exit **0**:

```sh
python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Canonical scoped parity reported `invention and status gates passed (156 rows, 198 public Rust items)`. Global exemption blocks: 1019; **WEEX exemption blocks: 0**. Extractor output was read: `Symbols: 7341 across 512 source files`; `Would update 0 manifests; 0 added, 0 orphaned symbols.`

The extractor writer completed twice with exit 0, each reporting two manifests updated and zero added/orphaned symbols. It preserves `rust_files`, so the corrected lists were derived programmatically from the real `rs` targets before the second writer normalized them; no file array was hand-spelled and no gate was weakened.

Only the control driver and declared manifest file lists changed after the native gate SHA. A scoped `git diff --exit-code` covering Rust source, Cargo definitions/lock, and all venue tests except `runtime-mutations.py` exited **0** against the native gate SHA. Those changes do not alter the compiled native test inputs; baseline exits remain attached to their measured SHA.

## Compiled production controls

`python3.12 sdk-rs/safe-cex-venue-weex/tests/runtime-mutations.py` completed with exit **0** at `007391dddaa4569e59046fad35df5c59292524f0`. Each control changed production Rust, compiled and ran its one named fixture test, restored the source bytes in `finally`, then reran the same target. Every broken execution exited **101** at the named native equality assertion, and every restored execution exited **0** with `1 passed; 0 failed`. No executed control survived. Enumeration stopped after control **9**, `income-page-delay`.

| Production control / call site | Named native assertion | Broken exit | Restored exit |
| --- | --- | --- | --- |
| repeat -> indices / `serialize_weex_params` | `runtime::native serialize_repeat` | 101 | 0 |
| emitted URL suffix / `create_api` | `runtime::native api_timeout` | 101 | 0 |
| public -> private child / `construct_exchange` | `runtime::native constructor_fields` | 101 | 0 |
| invert disabled guard / `validate_account` | `runtime::native validate_disabled` | 101 | 0 |
| wrong user-ID prefix / `get_account` | `runtime::native account_hash` | 101 | 0 |
| market -> ticker loaded flag / `start` | `runtime::native start_public` | 101 | 0 |
| swap position/balance values / `tick` | `runtime::native tick_success` | 101 | 0 |
| income cap 100 -> 99 / `fetch_income_history` | `runtime::native income_symbol` | 101 | 0 |
| page delay 100 -> 101 / `fetch_income_history_range` | `runtime::native range_short` | 101 | 0 |

Each execution used `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test runtime -- <named-test> --exact`, with the prescribed `SAFE_CEX_TYPESCRIPT_PATH`. Cargo processes ran serially. Control assertion filters require exactly one source fixture from the 42-fixture oracle; failures are runtime assertions, not compiler failures or trace-only assertions.

After all controls, working-tree and committed-tree comparisons against the control SHA covering venue `src`, `tests` and WEEX manifests both exited **0**. Logs and SHA-bound result JSON stay ignored under `sdk-rs/target/weex-runtime-gates/`; this manually authored evidence is tracked.

## Merge-down and integration-relative evidence

Fetched integration parent: `5c3c55b7b5631e9d04b3d31bc98b213b5888f18d`.
Merge SHA: `c037dff71cf6365910f57290df0c47decdc2829c`; merge exited **0**, with no conflicts.

Independent committed-manifest census against that actual integration parent again measured **8 pending -> ported, 1 -> divergent**, with 121 ported / 32 divergent / **3 pending** after merge. All nine moved rows again had real declared Rust targets; zero missing targets. This is the current integration-relative gain, not a stale merge-base count.

`git diff --exit-code 007391dddaa4569e59046fad35df5c59292524f0 HEAD -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests sdk-rs/parity/manifest/exchanges/weex` exited **0** at the merge SHA. Own source, tests and mappings survived byte-for-byte. The dependency audit found upstream additions to the shared crypto crate's Poseidon modules, changes to Cargo lock and the global exemption list. Workspace Cargo definitions, the WEEX crate definition, shared types/num/store/exchange/transport/candle-cache/parity crates, extractor code, WEEX TypeScript, shared pager/client-ID/type/utility/logging sources, package commands and yarn lock were unchanged.

Because a compiled dependency changed, all four baseline commands listed above were rerun serially at `c037dff71cf6365910f57290df0c47decdc2829c`. Measured exits: **cargo check 0; runtime test 0; scoped canonical parity 0; extractor check 0**. Runtime again reported **10 passed; 0 failed**, covering the 42-fixture floor. Canonical parity again passed 156 rows / 198 public Rust items, with 1091 global exemption blocks and **zero WEEX exemptions**. Extractor output was read: 7341 symbols / 512 source files; `Would update 0 manifests; 0 added, 0 orphaned symbols.`

The nine controls were **not rerun after merge**: their measured broken/restored exits remain bound to `007391dddaa4569e59046fad35df5c59292524f0`. Own controlled code and assertions were unchanged; shared pager logic and value/normalizer inputs were unchanged, and the post-merge native floor passed. The evidence update following the merge changes only this document.

## Scope and earlier findings

The oracle extracts actual TypeScript API declarations, exchange class, normalizers, client-ID pattern helper and shared pager. It uses the installed qs/Axios, Node crypto and actual enums. Transport/base/store/timer behavior is explicit fixture binding. Native receipts capture received request/factory arguments, results, updates, warnings, errors and initial eager effects. Native comparisons use decoded values and direct typed fields; there is no wire assertion phase.

The fixture host covers ordinary objects, bounded arrays, ASCII qs comparison keys, UTF-16 string values and binary64 bits. It is not a universal JavaScript runtime or Axios implementation. Arbitrary getters/proxies, descriptor changes/setters, mutable imported tables, all locale orderings, malformed body encodings, real transport/timers and complete class assembly have not been enumerated. Production exposes distinct assignment/field-definition and imported value-operation capabilities. Host factories/then/all must retain source work; the fixture `all` adapter observes fulfilled futures sequentially and **does not prove peer survival or rejection ordering**. No exact JavaScript microtask interleaving is claimed, including the typed pager's suspension adapter.

Read-only sub-agent review found assignment/read-order gaps and instrument gaps in GET argument receipts and the original tick-control anchor. These were corrected before measured control execution. Review ran no cargo, tests, mutations, edits or commits.

Preparatory failures: the first oracle invocation exited 1 on an unresolved qs path, fixed using the declared TypeScript dependency root. A scoped runtime attempt exited 101 for an unused import. At `7421f7777e83503f72c0184f8c2c9a695114276c`, runtime exited 101 (8 tests passed, 2 failed): named native assertions caught an initial 100 ms sleep before the Promise boundary in `income_reads.rs::eager` / `fetch_income_history_range`. The suspension barrier fix passed all ten tests at the native gate SHA. Canonical parity initially exited 1 at that SHA because new Rust files were undeclared; the generated declarations corrected it. The initial extractor check in that aborted bundle was **not run**.

An inverted testnet selector was rejected before execution as an equivalent candidate because both base URLs have the same value; the emitted-URL control replaces it. This is a static finding, not a measured survivor or a passed mutation gate.

Not run: full WEEX test suite, workspace compilation, full monorepo suite/build, host clippy, wasm clippy, live venue calls, CI reruns or PR creation/merge. The preliminary library-only check passed before the formal native gate; it is not a workspace compilation claim.
