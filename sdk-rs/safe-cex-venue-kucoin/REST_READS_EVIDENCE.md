# OKX and KuCoin REST read parity

Eight pending rows become ported, zero become divergent: six OKX and two KuCoin. Fresh branch feat/okx-kucoin-account-validation starts at actual integration parent `671cdadf56e6ec1c1955795f0976a8cfd0d28d09`, with no unreviewed feature branch used as parent. Source/mapping census SHA: `7db05e78fbb1036d5b8f83c13b9bcaa729908130`. Reviewed native-assertion correction SHA: `723edcf4b9f1c8436b2ec5f449351ca9fc6963e9`.

## Exact mappings and behavior

| TypeScript symbol | Rust target |
| --- | --- |
| OKXExchange.validateAccount | safe-cex-venue-okx/src/rest_reads.rs::validate_account |
| OKXEUExchange.validateAccount | safe-cex-venue-okx/src/rest_reads.rs::validate_eu_account |
| OKXExchange.fetchPositions | safe-cex-venue-okx/src/rest_reads.rs::fetch_positions |
| OKXExchange.fetchBalance | safe-cex-venue-okx/src/rest_reads.rs::fetch_balance |
| OKXExchange.fetchTickers | safe-cex-venue-okx/src/rest_reads.rs::fetch_tickers |
| OKXExchange.getOrFetchPositionModeAndAccLevel | safe-cex-venue-okx/src/rest_reads.rs::get_or_fetch_position_mode_and_account_level |
| KucoinExchange.validateAccount | safe-cex-venue-kucoin/src/rest_reads.rs::validate_account |
| KucoinExchange.fetchBalance | safe-cex-venue-kucoin/src/rest_reads.rs::fetch_balance |

Native bodies own endpoint/method selection, call/await order, validation guards, ordered result construction, live property extraction, cache guards and assignments, numeric field mapping, fallback branches, and catch boundaries. OKX swallows synchronous and rejected best-effort balance failures; KuCoin's native catch callback returns undefined for rejected balance promises while synchronous setup failures enter its outer catch. EU has no balance read. OKX/EU capture the emitter before error getters and repeat the getter chain after emission; KuCoin captures one computed error message. Cache checks stay outside its load catch, return fields are read live again, and mode assignment precedes numeric parsing/account-level assignment. A load failure captures emitter before the first error chain, then captures Error before the second chain.

Adapted capabilities own whole named live member/callee/receiver expressions, destructuring/iteration, intrinsic value construction, direct JavaScript parseFloat coercion and original throws. In particular KuCoin capture_request(BALANCE) captures both bound xhr.get and its first current ENDPOINTS.BALANCE argument before currency evaluation. parse_float/parse_field mean direct parseFloat(originalValue), including Symbol failures; replacing this with parseFloat(String(value)) violates the contract. Native fixtures cover the stated string/numeric/fallback cases, not a concrete provider's Symbol implementation. Futures are await handles over already initiated, noncancelling host promises; immediate initial polling represents JS async entry. Native code does not provide a concrete language/XHR/class/provider implementation. Those owning assemblies remain pending. No shared trait, shared type, crate dependency, or lockfile change.

Manifests were serialized programmatically with provenance unions. parity:extract writer ran after manifest edits, exit 0; output Updated 0 manifests / 0 added / 0 orphaned. KuCoin's explicit venue-oracle:kucoin script now includes --test rest_reads; OKX's existing all-target wiring includes it. Full venue scripts were not run.

## Actual gate exits

Cargo binary /opt/homebrew/opt/rustup/bin/cargo; every Cargo command ran sequentially with --jobs 2. Focused tests used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Actual SHA | Command | Exit |
| --- | --- | --- |
| 723edcf4b9f1c8436b2ec5f449351ca9fc6963e9 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| 723edcf4b9f1c8436b2ec5f449351ca9fc6963e9 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test rest_reads | 0 |
| 723edcf4b9f1c8436b2ec5f449351ca9fc6963e9 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 |
| 723edcf4b9f1c8436b2ec5f449351ca9fc6963e9 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test rest_reads | 0 |
| 7db05e78fbb1036d5b8f83c13b9bcaa729908130 | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

Both focused tests are named rest_reads_native_before_source. Extractor text inspected: 7341 symbols across 512 files, Would update 0 manifests / 0 added / 0 orphaned. Measured git diff --exit-code 7db05e78fbb1036d5b8f83c13b9bcaa729908130 to 723edcf4b9f1c8436b2ec5f449351ca9fc6963e9 over both manifest directories, both TypeScript venue directories, parity/extract-ts, both production source directories and package.json: exit 0. The earlier extractor remains measured at its actual SHA, not relabeled as a new execution. Fresh merge-head metadata gate follows below.

## Native assertions and witnessed floors

The independent literal floor is 65 fixtures: OKX 18 validators / 12 read delegations / 13 configuration reads; KuCoin 11 validators / 11 balance reads. Native decoded ordered fields, argument/callee/return/balance/cache/error/response identity, numeric bits, NaN, signed zero, captured endpoint, fresh Error name/message/identity, partial cache assignments and literal traces are asserted before any source comparison. Injected original native error assertions inspect TypeError name, transport message, READ code, ids [4,7] and original response Rc identity. Packing happens after each native fixture; the child source oracle runs only after all native assertions and literal count assertions. No decode/reencode roundtrip supplies proof.

Validation enumeration stops at EU emitter-reenter for OKX and api-code-empty for KuCoin; delegation stops at fetchTickers/result-error; configuration stops at emitter-reenter; balance stops at emit-error, including four-field iteration through unrealisedPNL (parse-error stops at orderMargin). These are finite witnessed floors, not exhaustive intrinsic/provider/async scheduling coverage. The maintained-source oracle extracts actual method bodies, actual endpoint tables, and actual PositionMode enum. It uses real Promise/catch, getters, direct parseFloat and Error semantics; native fixtures compare decoded values and identities first. KuCoin currency-reenter changes the live endpoint during argument evaluation and proves the already captured old URL remains the request argument.

Read-only sub-agent review found no production control-flow bug. It identified an emitted-value proof gap: both validators initially skipped native message/count assertions when the emitter itself threw. Commit 723edcf4b9 fixes that; emitted down and count one are now asserted before source comparison, with zero messages for error-read-error. Follow-up review was clear. Precommit unclosed test delimiter and unused fixture declarations produced compiler exit 101; both were fixed before the gates above. They are development failures, not mutation proof.

## Independent status census

Python 3.11 tomllib census at 7db05e78fbb1036d5b8f83c13b9bcaa729908130 versus actual parent 671cdadf56e6ec1c1955795f0976a8cfd0d28d09: exit 0. OKX 82 ported / 19 divergent / 62 pending becomes 88 / 19 / 56, exactly six pending-to-ported rows. KuCoin 103 / 39 / 49 becomes 105 / 39 / 47, exactly two. Each venue enumeration read 12 manifests and stopped at OKX okxeudemo.exchange.ts.toml and KuCoin kucoinspot.ws-public.ts.toml. This is a status census, not complete venue behavior proof. Fresh merge-head census follows below.

## Unmeasured scope

No workspace compilation, full venue suite/script, host clippy, wasm clippy, full Rust-index audit, CI rerun, live network test, concrete XHR/class/language/provider assembly, arbitrary custom promises/iterators/coercion hooks, Symbol-provider implementation, or exhaustive scheduling/cancellation behavior was run or proved. Already-ported rows outside this slice were not audited.

## Compiling family controls

At reviewed SHA 723edcf4b9f1c8436b2ec5f449351ca9fc6963e9, five source mutations compiled and failed the named native assertions at exit 101. Each source file was restored byte-for-byte, then the same venue rest_reads target passed at exit 0. Five family controls, not eight independent symbol controls. Enumeration stopped at KuCoin balance. Zero survivors; zero compiler failures.

| Venue / family / call site | Mutation | Named assertion | Broken / restored |
| --- | --- | --- | --- |
| OKX validation / validate_account | BALANCE route to ACCOUNT_CONFIG | VALIDATE native route | 101 / 0 |
| OKX delegations / fetch_positions | positions projection to balance | DELEGATE native property selection | 101 / 0 |
| OKX config / get_or_fetch_position_mode_and_account_level | Hedged branch returns oneway | CONFIG native decoded mode | 101 / 0 |
| KuCoin validation / validate_account | Success code 200000 to 200001 | VALIDATE native field order | 101 / 0 |
| KuCoin balance / fetch_balance | Swap free/used numeric source fields | BALANCE native independent numeric bits | 101 / 0 |

## Integration merge receipt

Fetched and merged integration `97b6f919d26396b52e4b3514f98fcfa34b4dcc6c` cleanly. Actual merge head: `f3964a5cb59832776dc93ed3af90ce43a6aef79a`. Original fresh parent remains 671cdadf56e6ec1c1955795f0976a8cfd0d28d09.

At that merge head measured `git diff --exit-code 723edcf4b9f1c8436b2ec5f449351ca9fc6963e9 HEAD --` both venue crates' src/tests/scripts/Cargo.toml, both manifest directories and package.json: exit 0, byte-unchanged. Separately measured workspace Cargo.toml/Cargo.lock, safe-cex-store (including the source loader), types/num/exchange/transport, both TypeScript venue directories, packages/safe-cex/src/types.ts, parity/extract-ts and yarn.lock: exit 0. Four scoped Cargo exits and five controls remain measured at their actual reviewed SHA, not relabeled as merge-head runs; no Cargo refresh was needed because tested inputs are unchanged.

Fresh `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` at f3964a5cb59832776dc93ed3af90ce43a6aef79a: exit 0. Inspected output 7341 symbols / 512 files; Would update 0 manifests / 0 added / 0 orphaned.

Fresh independent Python 3.11 tomllib census at that merge head against merged integration: exit 0, exactly six OKX and two KuCoin pending-to-ported changes, zero other status changes. OKX 82/19/62 becomes 88/19/56; KuCoin 103/39/49 becomes 105/39/47 (ported/divergent/pending). Each enumeration read 12 files, stopping at the same manifest named in the original census. Counts do not imply complete runtime proof.

Final narrow read-only sub-agent review found no receipt or preservation blocker at this merge head. It independently measured both preservation diffs at exit 0; it ran no Cargo/tests/index and made no edits. This delivery receipt edit changes only this manually authored evidence file.

Measured before receipt commit: `git diff --exit-code f3964a5cb59832776dc93ed3af90ce43a6aef79a -- . ":!sdk-rs/safe-cex-venue-kucoin/REST_READS_EVIDENCE.md"` exit 0; `git diff --check` exit 0. No source or gate inputs changed in the receipt edit.
