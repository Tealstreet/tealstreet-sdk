# OKX margin conversion parity

Five rows pending -> ported, zero -> divergent. This slice adds three KuCoin
refresh rows, for eight newly ported rows total. Branch
`feat/okx-kucoin-margin-refresh` was created directly from integration parent
`976c7bf08eb0d23329d7dfd9042035e3aabe2358`; no old feature branch was inherited.
Source and measured gate SHA: `fa166bc9d74bc0cf575d75b15234c3bf6d5b4542`.

Independent Python 3.11 tomllib comparison at this SHA covered all 12 OKX
manifests, stopping at `okxeudemo.exchange.ts.toml`: parent 59 ported / 19
divergent / 85 pending; source commit 64 / 19 / 80. Exactly five pending ->
ported changes and no other status changes were found. Census is distinct
from behavioral proof.

The five mappings target `src/margin_conversion.rs`: `#toFiniteNumber` ->
`to_finite_number`, `#getCtValCcy` -> `get_contract_currency`,
`convertMarginValueToUsd` -> `margin_to_usd`, `#convertUsdValueToMarginCurrency`
-> `usd_to_margin`, and `contractsToBase` -> `contracts_to_base`.

Host dependencies preserve complete String/coercion/uppercase and live member
expressions, including original errors. Pure truthiness distinguishes nullish
finite-number fallback from currency's falsy fallback. Full UTF16 currencies
remain native vectors. Unpaired surrogates are neither numeric-prefix units
nor JS whitespace; replacing them only for the parseFloat input cannot change
its accepted prefix. Parsing retains string '-0'; numeric -0 becomes String '0'.
Finite amount conversion precedes market/currency guards. Currency uppercase
precedes inverse; base/quote/reference reads remain lazy. Numeric price guards
use JavaScript truthiness. Inverse arithmetic uses plain product/division;
ordinary contracts use shared source safe-math multiply. Owning exchange
assembly and its external dependencies remain pending.

## Measured gates

All commands ran sequentially at `fa166bc9d74bc0cf575d75b15234c3bf6d5b4542`.
Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit |
| --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 0 |
| `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test margin_conversion` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 |

Extractor text: 7341 symbols / 512 source files, would update 0 manifests,
0 added and 0 orphaned symbols. The writer ran after manifest edits and also
reported zero updates/additions/orphans. Mapping notes and provenance were
serialized through the project manifest writer; no arrays were hand-spelled.

Native-first fixture floor: 288 = 24 finite parsing + 30 currency coercion +
168 margin conversions + 66 contracts-to-base cases. Native scalars, signed
zero bits, infinity or NaN classification, dependency argument values, complete
errors and original error identity are asserted before invoking the actual-source
oracle. Expected values are independent fixtures; fixed input counts are literal.
The oracle selects the maintained method bodies and shared safe-math source AST.

Enumeration stopped at Symbol coercion failure for finite parsing; surrogate
currency / coercion failure; USD-to-margin / Unicode uppercase expansion /
reference-price failure; ordinary infinite size / unused price failure. These
floors do not claim complete unknown-value or market enumeration.

Precommit test compilation exposed an ambiguous generic error type, fixed with
an explicit native Result type. Static review corrected a fixture expecting NaN
instead of infinity for ordinary infinite size. Neither is a production finding
or a discriminating mutation result.

Not run: workspace compilation, full venue suites, host clippy, wasm clippy,
full Rust-index check, CI, or live network/timer execution.

## Compiling controls

All five controls ran at source SHA `fa166bc9d74bc0cf575d75b15234c3bf6d5b4542`.
Each changed production bytes, compiled, exited 101 at the named native assertion,
restored exact bytes in finally, then reran the same focused cargo target and
exited 0. No control survived and no compiler failure was counted as a catch.

| Mutation | Named native assertion | Broken / restored exits |
| --- | --- | --- |
| Negate finite parsed-number predicate | `FINITE native scalar value` | 101 / 0 |
| Negate currency truthy fallback predicate | `CURRENCY native truthy fallback` | 101 / 0 |
| USD conversion multiply -> divide | `MARGIN-USD native scalar value` | 101 / 0 |
| Margin conversion divide -> multiply | `USD-MARGIN native scalar value` | 101 / 0 |
| Ordinary contract shared multiply -> add | `BASE native scalar value` | 101 / 0 |

Control enumeration stopped after contracts_to_base; the five controls are a
floor over these five newly ported bodies, not a complete mutation census.
Sub-agent static review verified fixes, mappings, module/oracle wiring and
callback scope; no remaining blocker. Reviewer ran no cargo or tests.

## Integration merge and fresh gates

Integration `ab593ef7504191a1cfd2bb094ef066aedd4c5c25` merged at
`3879259b71b536244e5d4f2d6069133048cb5688`, including the now-merged PR 7544.
Conflicts retained both module exports, both focused KuCoin test targets,
and exact per-symbol three-way manifest rows/provenance. The exemption union
was 567 ours / 585 incoming / 19 additions / 586 total; common entries/reasons
matched and duplicate targets were rejected. Its final ordering follows the
integration file with only our RefreshFuture alias appended, avoiding unrelated
entry relocation; the exact row set and reasons are unchanged.

All six new source/test/oracle files compared byte-identical to fa166bc9d7
with scoped git diff --exit-code 0. Whole venue inputs gained the prior slice's
public_lifecycle modules/tests and lib exports, so the cargo gates were refreshed
rather than treating whole crates as byte-unchanged.

All five commands in the two receipts' gate tables were rerun sequentially at
`3879259b71b536244e5d4f2d6069133048cb5688`: both scoped all-target crate checks,
OKX margin_conversion test, KuCoin private_refresh test, and extractor each
exited 0. Extractor text again reported 7341 symbols / 512 files and would
update 0, added 0, orphaned 0. The writer after manifest conflict resolution
also exited 0 and reported zero updates/additions/orphans.

Independent tomllib comparison at this merge SHA against integration ab593ef750
covered all 12 manifests per venue, stopping at the files named above: exactly
five OKX plus three KuCoin pending -> ported, zero other status changes.
Counts: OKX 68 / 19 / 76; KuCoin 83 / 39 / 69 (ported / divergent / pending).
Relative to original 976c7bf08e parent the delta is 16, comprising eight from
this slice and eight already merged in PR 7544; the latter are not counted
again as new work. Controls remain bound to their original source SHA.

Static sub-agent merge review verified the eight own rows, all other incoming
rows unchanged, exact provenance/exemption unions, exports and both oracle
targets; no blocker. It ran no cargo/tests.

## Delivery preservation

Final integration `c4c1ec196d794b431bed97b2ede5eff17a41c6f5` merged without
conflicts at `2438cfdba8733ecf93cbda292b65219d493101c0`. At that SHA, scoped
git diff --exit-code against measured gate SHA 3879259b71 exited 0 over both
venues' complete src/tests/scripts/manifests and package.json. A separate shared
Cargo/dependency source, TypeScript oracle, loader, safe-math, extractor source
and yarn.lock input diff also exited 0. Four cargo gate exits therefore stand
at their actual 3879259b71 measurement SHA; they were not relabeled or rerun.

Changed metadata gate ran at `2438cfdba8733ecf93cbda292b65219d493101c0`:
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`
exited 0, with 7341 symbols / 512 files and would update 0, added 0, orphaned 0.
Fresh independent tomllib census there against c4c1ec196d again enumerated all
12 manifests per venue, stopped at the same filenames, and found exactly
five OKX plus three KuCoin pending -> ported, zero other status changes.
Counts remain OKX 68 / 19 / 76, KuCoin 83 / 39 / 69. Only receipts change after
this measured delivery merge; no new behavioral or broader gate claim is made.
