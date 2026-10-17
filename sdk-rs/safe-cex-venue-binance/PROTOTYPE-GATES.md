# Binance shared prototype and native historical-value repair

This extends PR 7851. The original integration parent is
`ac0f5fccf24207b26c279e67da2a88f2c72a7aae`; the latest merge-down is
`f853d56fd227c2106dba609141b1fdd44bcab125`. The branch retains the earlier,
unmerged notification/owner work in this PR as an ancestor.

The shared statistics implementation now retains the builtin receiver and its
mutable fields in the socket's originating realm. Other sockets in that realm
observe those writes. Later reconciliation preserves own/inherited iteration,
partial-field presence, committed throttle on error and delete-before-error
cleanup. Realm bindings restore the caller after each future poll; pending
work never retains a thread-local binding.

Canonical historical mapping returns native values and builtin references.
The public `order_history::map_binance_historical_order` entrypoint and both
actual exchange Context paths call the same native implementation. The explicit
`map_binance_historical_order_typed` adapter runs that complete result before
checking the requested shared model. It retains a CHOSEN classification under
[the canonical output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary).
The former static-table copy is gone: inherited numeric 1000 is refused by the
typed adapter rather than incorrectly falling back to MARKET. No shared order
enum was widened, and no serialized convergence is claimed.

Native class Store capture also preserves Infinity, NaN and signed zero through
nested typed fields. Native JSON views distinguish callable omission from the
prototype object's own fields. Saved Spot callbacks retain their original realm
when a different realm is current, including a prototype-valued signature.

## Authored scope

Native assertions precede every new source comparison. The independent fixture
floors are 1280 statistics cases through case1279, 20 later/cross-socket sequences
through case19, and 39 historical records through case38. Six callable/object
field-policy assertions plus the numeric1000 typed-refusal assertion run before
the existing 4754-case typed comparison. The latter remains an opt-in typed
projection corpus; it is not presented as 4754 newly authored native cases.

The model covers the builtin/scalar property vocabulary reached by these source
operations. It does not execute arbitrary JavaScript functions or getters.
Canonical mapper arguments/results now use EventValue plus an explicit realm;
the closed typed projection has a distinct name. External Rust API compatibility
is unclaimed. Native alias identity checks are separate from JSON comparisons.

## Measurements

Scoped `cargo check --manifest-path sdk-rs/Cargo.toml --locked -p
safe-cex-venue-binance --all-targets --jobs 2 --keep-going` completed with exit 0
at `b294301e712763f6a90a1346944796b930576807`.

Three earlier compile exits101 remain recorded at their actual SHAs: db25983a35
needed a borrowed socket argument; 369b9c4e29 and 5b6c3f5a3b exposed old test
normalizers missing the new enum variant. They are compile failures, not killed
mutations. Existing native assertions now require real builtin identity and
originating realm. No authored fixture cases were removed. Explicit fixture
exclusions still panic instead of skipping/defaulting.

Source-only witnesses at 5b6c3f5a3b each exited0:20 prototype sequences, saved
callback/string signature, saved callback/prototype signature, and three exchange
constructors with nonfinite native Store values. These are not native Rust gates.
Source results include original/caller realm timestamps1000/2000 and a retained
signature field1000. Native positive and mutation results must be appended only
after their commands complete.

Broad workspace, host/wasm lint, both builds and doctest results remain attached
to the earlier SHAs in [INTEGRATION-GATES.md](INTEGRATION-GATES.md). They do not
cover this source. Whole-venue parity, arbitrary prototype behavior, CI reruns,
live exchange calls and browser execution remain unclaimed.

## Completed native gates and controls

Native target baseline at `b294301e712763f6a90a1346944796b930576807`
exited101 at the typed projection's present-undefined handling. This was a real
baseline failure, not a control. After repair, four native targets at
`8aa032b08fa61045fbd538354dc29c87167a1343` passed22/failed0/ignored0.
The added null/undefined/falsy typed-policy checks passed at
`d5a58c170aa641b3f7f78018b87ab71695f92786` (one test, exit0).
At `a7db1470500f16e55d102c0a46d715fb4ed0d3e3`, the four targets again
passed22/failed0/ignored0 and the shared websocket target passed53/failed0/ignored0.
The latter's independent source inventory is50 local test attributes plus3
included root tests, stopped at managed_relay_inline_discard_and_sleep_error_release_only_active_run
and included root_offline_recovery_native_state_table_and_timer_cancel.

The six authored Futures guard probes stop at string-status/used-weight1920:
missing fulfilled status, missing rejected status, string429, callable status,
numeric429 and string429 with used weight1920. Native identity/value assertions
precede Source. The new finite used-weight Source branch initially failed101
at `7d715f23052b00a13d2cfbb16b2bbbed5fef201c` because its fixture omitted
actual module constants. Loading the three real AST declarations restored the
owner test to exit0 at `86eda9cde784872be91c2e3036e9c642042e18f8`.
Native numeric retry time is22500; normal used-weight time is80000. No status
serialization or loose equality is introduced.

All20 distinct controls below ran at
`86eda9cde784872be91c2e3036e9c642042e18f8`:15 native mutations and5 source
corruptions. Each produced actual runtime101 at its named assertion, then the
same selected test passed with exit0 after byte restoration. No compiler101 is
counted. Every restored run has the empty tracked-diff SHA256. The final
`git diff --exit-code HEAD -- sdk-rs packages/safe-cex .github` exited0 after
all20 controls. This enumeration stops at source-private-request.

| Control | Named runtime assertion | Negative / restored exits |
| --- | --- | --- |
| equal-arity-identity | native exact inherited builtin identity before wire |101 /0|
| reconcile-commit | native failed reconciliation committed throttle before throw |101 /0|
| cleanup-delete | native early cleanup deletes own topic |101 /0|
| shared-socket-realm | native sockets share their captured realm |101 /0|
| queued-realm | native queued callback reads original table realm |101 /0|
| nullish-history | native historical type fallback preserves zero and suppresses eager fallback |101 /0|
| nonfinite-store | native owner Store projection retains positive Infinity |101 /0|
| builtin-object-projection | native prototype object keeps its object serialization view before wire |101 /0|
| typed-branch | native typed adapter refuses inherited numeric 1000 rather than falling back to MARKET |101 /0|
| typed-undefined-filter | native typed projection accepts undefined optional values |101 /0|
| typed-null-filter | native typed projection refuses present null instead of omitting it |101 /0|
| guard-status-serialization | native missing status fulfills unchanged without serialization |101 /0|
| guard-status-coercion | native string 429 does not select numeric retry branch |101 /0|
| guard-status-early-return | native nonnumeric status still observes used weight 1920 |101 /0|
| source-guard-status | actual string 429 does not select numeric retry branch |101 /0|
| source-wrapper | wrapper reaches actual core stats |101 /0|
| source-shared-write | source writes shared prototype timestamp |101 /0|
| source-historical | pin inherited constructor bug |101 /0|
| private-captured-realm | native signer request inherited timestamp is numeric |101 /0|
| source-private-request | source saved Spot open constructs signer request in original realm |101 /0|

The first restored executable paused before its test harness in macOS dyld and
later completed. A read-only OS sample and code-sign verification were diagnostics,
not Rust gates; no security settings or caches were changed.

The typed projection also checks its canonical13 own fields, optional None and
populated Some values4.2/3.6/2/LONG, signed zero/zero/false, and present-null refusal
before the Source corpus. Removing its undefined filter and broadening it to
nullish both fail different named assertions. Its inherited-number1000 control
also fails before Source. The queued realm control observes Number1000 versus
Undefined; the equal-arity identity control observes Constructor versus HasOwnProperty.

## Limits and next gates

The owner rows retain FINDINGs for generic Exchange dispatch and ownership
cycles. Futures additionally lacks a production imported HTTP client path retaining
executable header.get and rejected-response getter provenance. The guard status
fix and nonfinite Store repair do not close those findings. Generic binding must
also address fallible listeners/cleanup and resolve-before-post-hook worker
sequencing; returning a no-op or rejecting an already-resolved worker result
would change the source channel.

The completed gates above do not cover final workspace lint/build/doctest results.
Host clippy and wasm clippy remain separate measurements. The full Binance suite
is also still owed for this source. Its independent expected test count is289:
281 literal integration test attributes plus8 extra expansions from the nine
message-test invocations sharing one macro template;72 top-level integration
files stop at types.rs, plus the empty library test target. This is an expectation,
not a passing result. No whole-venue parity claim, CI rerun, browser execution,
live venue request or external Rust API compatibility claim is made.

## Manifest reconciliation

After the completed controls, five Binance statistics rows and three shared-core
statistics rows moved from divergent to ported. The historical row remains
CHOSEN divergent; only its inherited-object FINDING is closed. The owner Store
nonfinite subfinding is removed, while all three owner rows retain their other
FINDINGs. TOML census:268 ported /18 divergent /0 pending,286 rows across20 files,
stopped binancespot.ws-public.ts.toml. Relative to the previous notification
slice, this is5 additional Binance rows ported and3 shared-core rows ported.
Across this PR, the16 legacy socket findings move to ported; whole-venue parity
remains unclaimed.

Structured parse/serialize reconciliation repairs file provenance in13 existing
Binance manifests and adds order_history_native.rs to the historical manifest.
Every declaration derives from its current mapped targets and names an existing
file. This metadata census does not prove symbol resolution or semantics; the
actual Rust status/invention check remains owed. No row target is invented.
The canonical manifest writer at implementation86eda9cde7 plus the recorded
metadata working diff exited0; output reported Updated0 manifests,0 added and
0 orphaned. The committed-tip freshness check will be measured separately.

## Subsequent adversarial finding and corrected labels

A later sub-agent review identified an inherited-length state outside the earlier
1280/20 fixture floors. The canonical remaining scope and observed versus
code-inspected outcomes are recorded only at
[diagnostic prototype scope](../parity/MANIFEST-SPEC.md#diagnostic-prototype-write-choice).
The three core rows and five Binance forwarding rows have been reconciled back
to FINDING; their previous PORTED labels exceeded that evidence. The earlier
positive gates and twenty controls retain their measured SHAs and bounded
fixture floors. They do not close this new state.

The portable actual-source witness is
[prototype-length-source.cjs](gate-evidence/prototype-length-source.cjs). Its
native counterpart, native failure control and repair remain owed. No native
result for this new witness is claimed. PR7851 remains draft while the urgent
formatter CI repair is isolated on a fresh integration branch.


### Unused mapper cleanup

The separate closed `map_binance_historical_order_typed` adapter and its dedicated
`tests/order_history.rs` target were removed. Historical source evidence remains
in `tests/history_native.rs`; both production history contexts continue through
the canonical native mapper. The earlier typed-policy measurements above describe
the original tree only. Event tests now consume the production native rejection
classifier; the unused JSON-domain `private_order_logic` copy is removed.
