# Polymarket REST forwarding bodies

**10 pending → ported; 0 pending → divergent.** Polymarket triple: **34 / 18 / 89** (ported / divergent / pending). [Census](evidence/rest-forwarding/census.json) compares the manifests to requested parent `473004e7b056eacb8bb51419d062993624c4e908`. Census is not runtime proof.

The fresh branch `feat/polymarket-rest-forwarding-473004` starts directly at that parent. No private/unreviewed lane work was introduced as an ancestor. Before gates, integration `c54039795784efc67fbfa0bb7d0109e47440ee11` was merged down. Before delivery, integration `d8c841d4c02209793adfbd58522f3db1316488ce` was also merged down.

## Implementation and scope

The ten original method bodies are `getSimplifiedMarkets`, `getSpread`, `getGammaMarket`, `getGammaMarketBySlug`, `searchGammaMarkets`, `cancelOrder`, `cancelOrders`, `cancelMarketOrders`, `cancelAllOrders`, and `deleteApiKey`. Each has an individual callable Rust function in src/rest_forwarding.rs and an explicit manifest target. MethodRequest/MethodResponse/MethodData represent the anonymous source request and response-data shapes; their exact targets and roles are explained in the mapping notes. The class, constructors and client factory stay pending.

The prepared request records source receiver selection (`xhr` versus `gammaXhr`), verb, endpoint, params and data before Axios transforms. Identifiers and URL paths use UTF-16 code units, retaining lone surrogates and the source's unescaped concatenation. Cancellation arrays retain the live original shared array. Read bodies forward ordinary data unchanged; delete bodies await completion and return void. Generic data and errors retain original native identities with no typed-schema refusal or error rewriting.

The injected host starts and independently owns each operation. Method futures borrow its pinned completion future. They invoke the host eagerly before polling, remain pending until completion, and releasing an unpolled or pending observer cannot drop the operation. This preserves source Promise observation semantics at the declared boundary; client startup, autonomous scheduling, authentication, wire transforms, real network requests and root-thenable assimilation are outside this slice.

## Gates bound to a commit

Scoped Cargo, tests, formatting, official target checker, extractor and mutation control exits below were measured at **`3c741672c0fb6c19634b21763728ae406661cf1d`**. Exact commands/exits and raw output are tracked under [evidence/rest-forwarding](evidence/rest-forwarding/gates.json).

| Gate | Measured exit and result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --all-targets --locked --jobs 2` | 0 |
| Scoped Polymarket tests with actual TS compiler environment, `--locked --jobs 2` | 0; 32 passed, 0 failed; doc tests ran 0 cases |
| Scoped cargo fmt check | 0 |
| Official Polymarket-only invention/status checker | 0; 141 rows, 62 public Rust items; empty venue exemption file |
| Actual Yarn `parity:extract:check` | 0; inspected output: would update 0 manifests; 0 added/orphaned symbols |
| Ten call-site control script | 0; shared 11-test baseline0, each compiled isolated fault101 and restored exact-test0 |

The dirty-draft focused run passed 11 tests before the implementation commit. That development run is not a gate covering this SHA; the committed full venue run and controls establish the measured results above.

## Native evidence and discriminating controls

The oracle extracts and executes the actual ten TypeScript method bodies and original ENDPOINTS declaration using the isolated installed compiler. Recording request boundaries expose receiver, verb, native string units, field keys, presence and cardinality. Source captures are compared alongside independent native expectations, so changed TS receiver/verb/key/count cannot silently pass a hardcoded Rust expectation.

Native tests assert prepared request fields, opaque response/error identity, actual native payload fields, ordered collection counts, NaN/negative-zero Number values, eager dispatch/pending completion, and two-way response/cancellation-array alias mutations. No native decode/re-encode, serde roundtrip, or serialized native JSON comparison is used as a value assertion. Source Number bits are captured before oracle JSON output.

| Isolated production URL mutation call site | Named failed native assertion | Baseline / mutated / restored exits |
| --- | --- | --- |
| `getSimplifiedMarkets` | `native request URL:getSimplifiedMarkets` | 0 / 101 / 0 |
| `getSpread` | `native request URL:getSpread` | 0 / 101 / 0 |
| `getGammaMarket` | `native request URL:getGammaMarket` | 0 / 101 / 0 |
| `getGammaMarketBySlug` | `native request URL:getGammaMarketBySlug` | 0 / 101 / 0 |
| `searchGammaMarkets` | `native request URL:searchGammaMarkets` | 0 / 101 / 0 |
| `cancelOrder` | `native request URL:cancelOrder` | 0 / 101 / 0 |
| `cancelOrders` | `native request URL:cancelOrders` | 0 / 101 / 0 |
| `cancelMarketOrders` | `native request URL:cancelMarketOrders` | 0 / 101 / 0 |
| `cancelAllOrders` | `native request URL:cancelAllOrders` | 0 / 101 / 0 |
| `deleteApiKey` | `native request URL:deleteApiKey` | 0 / 101 / 0 |

All ten mutants compiled and reached their named runtime assertion. Each source mutation was restored in a finally block; final source byte comparison passed. [Control summary](evidence/rest-forwarding/controls/summary.json) and 21 raw logs record the shared baseline and individual broken/restored runs. No survivor was observed in this ten-control floor. These are ten separate faults, not a combined mutation.

Independent expectations: ten methods × nine fixtures × two outcomes = **180** source cases, **18** per method. Fixed fixture counts and known per-case 0/1/3 request-field/array cardinalities are independent of produced artifacts. Nine response shapes include ordinary object, null, false, zero, string, array, undefined, NaN and negative zero. Identifier inputs include blank, astral text, lone high/low surrogates, punctuation, duplicate IDs and embedded NUL. The oracle is captured once per test process. The source/native corpus stops at `deleteApiKey`, fixture **8**, failure (**case179**). Each method stops at its eighteenth case, fixture8 failure. The two native observer-abandonment cases stop at `polled=true`; source ignored-promise completion has a separate fixed one-operation expectation. Mutation enumeration stops at `deleteApiKey`. Manifest enumeration stops at `polymarket.ws-public.ts.toml`, the seventh venue file. These are coverage floors, not a complete venue census of proved runtime behaviors.

A sub-agent reviewed implementation, source bodies, tests, oracle and mappings. Its operation-ownership and unchecked source-capture findings were fixed before measured gates; getter scope explicitly excludes thenables. Final review found no remaining actionable issue. The reviewer ran no gates or controls.

## Merge preservation and non-claims

[Preservation receipt](evidence/rest-forwarding/preservation.json) compares measured SHA `3c741672c0fb6c19634b21763728ae406661cf1d` with delivery merge head `f9a0fbf325a29ee414bef38a84833d524af5e012`. Required Polymarket src/tests comparison and seven dependency src/Cargo.toml comparisons exit **0**. The broad ninth comparison exits **1** solely because incoming **foreign exemptions** changed in global exempt.toml; this exit is retained along with the exact diff. A split follow-up comparison of Cargo.lock, root Cargo.toml, checker/extractor and TS source exits **0**. Independent Polymarket exemption expectations are **0** before and after; the scoped checker supplies an empty exemption file, so foreign exemptions do not alter its gate inputs. No relevant Cargo/test/target-check inputs changed, and the original measured exits stand; no freshly rerun Cargo gates at the merge head are claimed.

A fresh global extractor check at exact merge SHA `f9a0fbf325a29ee414bef38a84833d524af5e012` exited **0**, with zero manifest changes; see final-extractor.json/log. Subsequent evidence-only commits do not alter gate inputs.

No full workspace build/test, host clippy, wasm clippy, CI or MEXC gates were run for this Polymarket slice. No constructor, authenticated HTTP client, whole class, response thenable, or real-network completion proof is claimed. The remaining 89 Polymarket rows stay pending.

Last pre-push fetch/merge includes integration `19cb1a2b735c3bc6db40bbb0cee849121cfd985b` at comparison head `9521a7b1b6eaa989e65d94105884a91b78210f69`. [Last receipt](evidence/rest-forwarding/last-merge-preservation.json) records relevant source/test/dependency/target inputs unchanged (exit0); prior scoped gates therefore stand at their measured SHA. Global extractor handling at this last head is recorded separately in that receipt.
