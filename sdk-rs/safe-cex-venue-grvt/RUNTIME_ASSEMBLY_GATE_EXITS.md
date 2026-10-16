# GRVT trading, signing and runtime assembly evidence

Implementation/gate SHA: `58b535aa9cd2d5fdd2e79dd92b8ba479e0a88bc7`.
Fresh branch: `feat/rust-grvt-runtime-assembly`.
Actual starting parent: `46cba6b45913ba58a39ba4a65fac1ad3bab9ad4b`, the merged integration head requested by Sam. No unreviewed feature branch was used as an ancestor.

Independent Python 3.14 tomllib census against that parent: **10 rows moved: 7 pending → ported, 3 pending → divergent (CHOSEN)**. GRVT changes from 106 ported / 48 divergent / 10 pending to **113 ported / 51 divergent / 0 pending**, across 164 rows. Zero pending is a census, not proof of complete venue/network parity.

The ported rows are placement, fill reconciliation, legacy signing, private-key validation and the three constructors. The three class rows explicitly choose native owned composition and required instance-bound member factories in place of JavaScript inheritance/prototype reflection and arbitrary untyped collection replacement. Each class has its own bounded note and actual Rust owner target. Concrete transport, SDK and complete member-adapter wiring remain outside this claim. No shared type or trait was changed.

## Measured gates at the implementation SHA

| Command | Measured exit / output |
| --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --all-targets --jobs 2` | 0; finished dev profile, zero errors |
| Same cargo executable, `test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --test runtime_assembly --jobs 2`, Node 24.20.0 | 0; 7 passed / 0 failed |
| Same scoped test with `PATH=/opt/homebrew/opt/node@26/bin:$PATH`, Node 26.7.0 | 0; 7 passed / 0 failed |
| `NODE_PATH=$PWD/sdk-rs/target/grvt-extract-deps/node_modules node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 at post-merge SHA `c082ac0ccab0585fd744760ccacd38c35fbcf41f`; **Would update 0 manifests; 0 added, 0 orphaned symbols** |
| `git diff --exit-code 58b535aa9cd2d5fdd2e79dd92b8ba479e0a88bc7 -- sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests` after all controls | 0; exact restoration |
| `git diff --check` | 0 |

The scoped tests launch the actual TypeScript source oracle using `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Bodies come from the original AST declarations/field initializers without body rewrites. Imports and external operations are bound explicitly. Native decoded values, error identities, field order, receiver/handle identities and effects are asserted directly; no encode/roundtrip comparison is a gate. Nonfinite classification is asserted; this slice does not claim NaN payload-bit equivalence.

The writer ran before implementation commit `38fd24ecb2b77fdc435efd90f6e68fd61012d039`: measured exit 0 and **Updated 4 manifests; 0 added, 0 orphaned symbols**. The four resulting manifest files were committed there. The follow-up commit changes only retained Map-view test identity assertions. Yarn results were judged from output text.

## Independent source case floors and stopping points

| Family | Fixed floor | Last case reached |
| --- | ---: | --- |
| Placement | 37 | number-string-error |
| Fill reconciliation | 29 | iterator-append |
| Legacy signing | 8 | receiver |
| Wallet/scalar validation | 8 | null |
| Exchange constructor | 15 | rules |
| Public constructor | 4 | member-binding |
| Private constructor | 4 | member-binding |

Total floor: 105 cases. These are bounded witnesses, not exhaustive inputs. Constructor own-declaration floors are independently fixed at 53 exchange, 27 public and 37 private; method floors are 43 exchange including 6 prototype members, 21 public and 22 private. The canonical client-ID rule reaches the seventh witness `-1`. Native retained-view probes additionally cover nonce IDs/times, reverse nonce Map writes and Map.set receiver identity, ticker IDs, timers and processed fills. Private inherited disposal/ping cells are probed in both representations. Factory assertions check the actual owner/class passed to each member factory; the two invoked socket member bodies are independent of `this`, so they do not prove complete owner-sensitive callback integration.

## Discrimination controls at the implementation SHA

Each control temporarily changed the production file, ran only its named test with `-- --exact`, restored the exact bytes in a `finally` block, and reran that same test. All mutants compiled and reached their named native assertion. Every mutated run exited **101** and every restored run exited **0**. No survivors.

| Family / named test | Deliberate change | Named assertion reached |
| --- | --- | --- |
| placement_native_source | Signed leg buys when side is SELL instead of BUY | `place/limit ordered effects native bool` |
| fills_native_source | Double the accepted fill amount before aggregation | `fills/fallback-account native number` |
| signing_native_source | Read expiration into returned nonce | `sign/default native string` |
| private_key_native_source | Invert Wallet-construction success predicate | `key/valid-prefix native scalar validity` |
| exchange_constructor_native_source | Default leverage 11 instead of 10 | `exchange/bare native number` |
| public_constructor_native_source | Initialize the owned request counter to 2 instead of 1 | `public/default native number` |
| private_constructor_native_source | Initialize the owned request counter to 2 instead of 1 | `private/default native number` |

The first mutation script invocation rejected an ambiguous selector before editing or running cargo (script exit 1). Its selector was narrowed to the signed leg. It is not counted as a discrimination control.

Earlier uncommitted development runs exited 101 for compile/fixture defects: unused import, fixture Debug/TableHit issues, null/undefined and decimal grammar, incomplete live-view construction, async rejection timing and a missing retained builder-error fixture. Those working trees have no immutable implementation SHA; they certify no commit. The failing fixtures were corrected before the SHA-bound gates above. The first committed seven-family Node 24/26 runs at `38fd24ecb2b77fdc435efd90f6e68fd61012d039` both exited 0; the stricter retained Map.set probe was subsequently added and both runs repeated at the implementation SHA.

## Sub-agent review and limits

Read-only production review by `/root/review_order_helpers` and fixture/manifest review by `/root/review_snapshots` are bound to `58b535aa9cd2d5fdd2e79dd92b8ba479e0a88bc7`. Their concrete findings were corrected, including explicit undefined defaulting, radix 10, numeric toString calls, retained collection views, undefined health, member-factory inputs and builder-error identity. They reported no remaining blocker within the declared scope and ran no gates themselves.

Not run: workspace compilation, full crate/monorepo suites, host clippy, wasm check/clippy, CI reruns, live-exchange/network tests, browser tests or complete concrete runtime-adapter integration. Host and wasm are separate claims; only the scoped host check above was measured. Prior GRVT suites were not rerun; existing runtime bodies were unchanged apart from a public-state documentation correction.

## Merge-down preservation

Merged integration `acf29148c3f26a07b72f93d1d964b49e81016ec6` into this branch, producing `c082ac0ccab0585fd744760ccacd38c35fbcf41f`. Three independent `git diff --exit-code 58b535aa9cd2d5fdd2e79dd92b8ba479e0a88bc7 HEAD -- <scope>` commands each exited 0 for: (1) GRVT src/tests, (2) GRVT manifests, and (3) Rust Cargo.toml/Cargo.lock and the exchange/store/crypto/num/types/transport crates, plus TypeScript GRVT/types/safe-math, logging severity and SDK signing type inputs. The implementation and its measured input scopes are byte-identical, preserving the scoped host/test/control evidence. Cargo was not rerun after this unchanged merge. The extractor was rerun at the merge SHA with the exit/output shown above. The earlier extractor invocation overlapped control edits and is superseded by this clean post-merge run.

Additional transitive-input comparison at `c9e65ca1ea6b8e22f10c0335a07ee93f4ea8cf22`: `git diff --exit-code 58b535aa9cd2d5fdd2e79dd92b8ba479e0a88bc7 HEAD -- sdk-rs/safe-cex-candle-cache sdk-rs/.cargo .cargo package.json yarn.lock` exited 0. Candle-cache depends only on the already-compared native num/types crates and locked external dependencies. Subsequent evidence-only commits do not change the measured implementation or test inputs.

The pre-push fetch advanced integration to `23202deb89227f307456132ecb2a2e2de6a22f37`; merged down again, producing `5798f3dd241cda4ef6769a879797a027f7ba4564`. Repeated the three independent scoped comparisons (with candle-cache/config/package/lock inputs included in the dependency scope): all exited 0. Repeated the extractor at this merge SHA: exit 0, **Would update 0 manifests; 0 added, 0 orphaned symbols**. Repeated the independent tomllib comparison using committed manifests at this SHA against the actual starting parent: exit 0, 7 pending→ported / 3 pending→divergent, 113 ported / 51 divergent / 0 pending. The subsequent reporting commit changes this evidence file only; no test, source or manifest mutation remains.
