# KuCoin private refresh parity

Three rows pending -> ported, zero -> divergent. With five OKX helper rows,
this slice adds eight newly ported rows. Branch `feat/okx-kucoin-margin-refresh`
was created directly from integration parent
`976c7bf08eb0d23329d7dfd9042035e3aabe2358`; no old feature branch was inherited.
Source and measured gate SHA: `fa166bc9d74bc0cf575d75b15234c3bf6d5b4542`.

Independent Python 3.11 tomllib comparison at this SHA covered all 12 KuCoin
manifests, stopping at `kucoinspot.ws-public.ts.toml`: parent 76 ported / 39
divergent / 76 pending; source commit 79 / 39 / 73. Exactly three pending ->
ported changes and no other status changes were found. Census is distinct
from behavioral proof.

Mappings in `src/private_refresh.rs`: futures `#refreshSnapshotSoon` ->
`refresh_snapshot_soon`, spot `#refreshOrdersSoon` -> `refresh_orders_soon`,
and spot `#refreshBalanceSoon` -> `refresh_balance_soon`.

The snapshot path clears a truthy prior handle, reschedules for 500ms without
an entry disposal guard, and assigns the new handle only after scheduling.
Its firing clears the current slot before checking disposal and synchronously
invoking the snapshot capability; the host retains ignored-promise semantics.
Spot paths check pure timer truthiness before disposal and schedule for 250ms.
Firing resets the current slot, checks disposal and invokes fetch immediately,
before polling the returned future. Host fetch capabilities convert synchronous
getter/invocation throws into rejected futures. Balance completion rechecks
disposal before the complete live store.update / current loaded-spread expression.
Cloned handles retain primitive value or opaque reference identity. The host
owns actual timer/promise scheduling; owning websocket/store assembly is pending.

## Measured gates

All commands ran sequentially at `fa166bc9d74bc0cf575d75b15234c3bf6d5b4542`.
Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit |
| --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0 |
| `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test private_refresh` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 |

Extractor: 7341 symbols / 512 files, would update 0, added 0, orphaned 0.
Writer followed metadata edits and reported zero updates. The exact RefreshFuture
support alias has one C3 exemption. The explicit KuCoin oracle command includes
the new focused target; the whole venue command was not run.

Native-first floor: 42 scenarios = 14 for each of snapshot/orders/balance.
Native assertions pin clear/schedule/firing/await order, exact 500/250 delays,
old and new timer identity, original errors and identity, immediate reset/fetch
before future polling, a real pending-await boundary, original decoded balance
identity and pointee, and live loaded fields. Source callback/method bodies are
selected from maintained TypeScript AST and native assertions precede all source
or wire comparisons. Counts and traces use independent literal expectations.

Scenarios include absent/falsy/truthy handles, early and later disposal, clear,
schedule and fetch failures, inline firing, reentrant fetch, replaced timer slots,
and loaded-state mutation while balance is awaiting. Enumeration stopped at
balance / superseded timer; this is a finite fixture floor, not a full timer
or event-loop enumeration.

Precommit compilation exposed test closure lifetime/type inference errors;
they were fixed before the source commit. Static review added old clear-handle
identity assertions and verified the callback pre-await prefix timing. These
are test refinements, not discriminating production mutation results.

Not run: workspace compilation, full venue suites, host clippy, wasm clippy,
full Rust-index check, CI, or live network/timer execution.

## Compiling controls

All three controls ran at source SHA `fa166bc9d74bc0cf575d75b15234c3bf6d5b4542`.
Each changed production bytes, compiled, exited 101 at the named native assertion,
restored exact bytes in finally, then reran the same focused cargo target and
exited 0. No survivors and no compiler failures counted as catches.

| Mutation | Named native assertion | Broken / restored exits |
| --- | --- | --- |
| Snapshot delay 500 -> 250 | `REFRESH native schedule order/delay` | 101 / 0 |
| Orders delay 250 -> 500 | `REFRESH native schedule order/delay` | 101 / 0 |
| Remove post-await balance disposal guard | `REFRESH native forbidden balance update` | 101 / 0 |

Control enumeration stopped after refresh_balance_soon; this is a three-control
floor, not a full timer mutation census. Sub-agent static review verified old
clear-handle identity, immediate firing prefix, mappings, exact exemption and
oracle wiring; no remaining blocker. Reviewer ran no cargo or tests.

## Integration merge and fresh gates

Integration `ab593ef7504191a1cfd2bb094ef066aedd4c5c25` merged at
`3879259b71b536244e5d4f2d6069133048cb5688`, including the now-merged PR 7544.
Five conflicts retained both lib exports, both KuCoin test targets, the exact
per-row manifest three-way changes and provenance union. Exemption union:
567 ours / 585 incoming / 19 additions / 586 total, all common entries/reasons
identical and no duplicates. Final exemption ordering preserves integration's
file and appends only our RefreshFuture entry, with identical rows/reasons.

Scoped git diff --exit-code for all six new source/test/oracle files against
fa166bc9d7 exited 0. Whole crates gained prior public_lifecycle modules/tests
and lib exports, requiring fresh cargo gates rather than whole-crate preservation.
At `3879259b71b536244e5d4f2d6069133048cb5688` the five commands across these
two receipts' gate tables reran sequentially: both scoped all-target crate checks,
both new focused tests, and extractor each exited 0. Extractor: 7341 symbols /
512 files, would update 0, added 0, orphaned 0. Writer after manifest conflict
resolution also exited 0 and reported zero updates/additions/orphans.

Independent tomllib comparison at this merge SHA against integration ab593ef750
covered 12 manifests per venue and stopped at the files named above: exactly
three KuCoin plus five OKX pending -> ported, no other status changes.
Counts: KuCoin 83 / 39 / 69; OKX 68 / 19 / 76 (ported / divergent / pending).
Against original 976c7bf08e parent the delta is 16: eight newly ported here plus
eight inherited from merged PR 7544. The previous slice is not counted as new
work. Controls remain tied to fa166bc9d7. Static sub-agent review verified the
merge union, all eight own rows, incoming rows, provenance, exports and targets;
no blocker and no cargo/tests run by reviewer.

## Delivery preservation

Final integration `c4c1ec196d794b431bed97b2ede5eff17a41c6f5` merged without
conflicts at `2438cfdba8733ecf93cbda292b65219d493101c0`. Scoped git diff
--exit-code against measured 3879259b71 gate SHA exited 0 for both venues'
complete src/tests/scripts/manifests and package.json. Separate shared Cargo,
dependency source, TypeScript oracle, loader, safe-math, extractor source and
yarn.lock input comparison also exited 0. The four cargo exits still stand
at their actual 3879259b71 measurement SHA; no relabeling or cargo rerun.

The changed metadata gate at `2438cfdba8733ecf93cbda292b65219d493101c0`,
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`,
exited 0: 7341 symbols / 512 files, would update 0, added 0, orphaned 0.
Independent tomllib census at that SHA against c4c1ec196d covered 12 manifests
per venue, stopped at the same named files, and found exactly three KuCoin plus
five OKX pending -> ported with no other status changes. Counts remain
KuCoin 83 / 39 / 69 and OKX 68 / 19 / 76. Only these receipts change after the
delivery merge; no broader or fresh cargo gate claim is made.
