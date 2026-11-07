# KuCoin local Read terminal authority

Frozen original-source base: `661427d10b3747927198e52ca239c4a5d4601826`.
Plan before tests and production. #1888 remains first after #1869; this RC slice
must integrate actual #1888 before final current-landing performance/CI proof.

## Selection, ownership and exact inventory

The largest currently clear coherent area is eight cold local Read terminal
expressions: five Spot query exits, Spot FetchAccount's missing-credential refusal,
Spot FetchLeverage's constant setting projection and futures FetchLeverage's
unsupported refusal. This exceeds the two clear BloFin account preflights and
other isolated local sites. The four pending venue slices own KuCoin history/
funding, Bitrue history/account and Bitunix history; their files remain untouched.
BloFin history/lib/tests remain #1865's; Bitunix lib/account/socket files remain
#1771's. Remaining private/signing/Session/group/Plan paths keep their prerequisites.

Fresh open-PR lists show no selected source/new-test collision. HMAC owner
claude-4yjlw0 cleared Spot mod/trading and futures lib; session owner claude-55ao7t
cleared futures lib, retaining only its future bullet region. No private/bullet,
request/signing helper or credential-construction changes are proposed.

Full frozen KuCoin inventory80 occurrences/70 rows/nine files becomes72/63/nine.
Spot mod16->11 occurrences (14->10 rows); Spot trading12->10 (11->9 rows);
futures lib10->9 (8->7 rows). Exact prune8 occurrences/seven rows only. Preserve
incoming rows and independently re-apply #1901's16-occurrence prune if it lands;
this plan does not claim its removals or whole-venue zero.

| Frozen source | Fingerprint | Original expression |
| --- | --- | --- |
| `venues/kucoin/src/spot/mod.rs` | `97e495223a50ee0b0214ca3f811598e16b758b9bb3eb780db321e78fc0c17442` | `cx . fail ( command , "KuCoin Spot requires credentials" )` |
| `venues/kucoin/src/spot/mod.rs` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | `cx . done ( command , "[]" . into ( ) )` |
| `venues/kucoin/src/spot/mod.rs` | `06eab8c4791cec49c676233d2e64048f1eaefe972dddfc52dfdffa06b4240730` | `cx . done ( command , serde_json :: to_string ( & Found { found : true , order : o , } ) . unwrap ( ) , )` |
| `venues/kucoin/src/spot/mod.rs` | `96134b6210eed5e21fe96c0412c596ffa47622ce39c4f6247960efe6dec683a2` | `cx . fail ( command , "KuCoin Spot public trades are not implemented in TS" , )` |
| `venues/kucoin/src/spot/mod.rs` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | `cx . done ( command , "[]" . into ( ) )` |
| `venues/kucoin/src/spot/trading.rs` | `d8877910dd4935bc942e96557431f8cb07f3f22f072bc567edd25ba564f4dcf9` | `cx . done ( command , "1" . into ( ) )` |
| `venues/kucoin/src/spot/trading.rs` | `97e495223a50ee0b0214ca3f811598e16b758b9bb3eb780db321e78fc0c17442` | `cx . fail ( command , "KuCoin Spot requires credentials" )` |
| `venues/kucoin/src/lib.rs` | `af50b0be7d7a0e63d0686b381091e4fd428fb7b4980c453864ee995b501543b0` | `cx . fail ( command , "KuCoin leverage query is unsupported" )` |

## Shared API shape and caller contract

Declare a pure crate-local policy constructor: Read, credential None,20,000ms,
no pacing, Never retry. Each returning local branch directly admits with
admit_http_command inside its genuine initial callback. Rejected/Revoked return
without numeric fallback or another terminal. No owner is acquired on a valid
signed fill, user-info or missing-cache pager fallthrough.

The command-entry owner explicitly confirmed None-credential Read authority for
cached Store lookup and Spot constant1 leverage projection on the frozen source.
None declares no transport credential or route; these branches read no private
remote data and lend no authority to later I/O. Retain the credentials-presence
check before all Spot Query matching. Preserve exact cache symbol/client/native
ID match rules, excluding empty/0/undefined client IDs, and the exact found/order
JSON. Serialize the borrowed cached Order into an owned result before mutable Cx
admission to avoid extending a Store borrow. Check the original Read root with
validate_read_root_context immediately before result publication.

Spot FetchLeverage admits and validates that original root immediately before
its existing local Store projection: leverage1, buy/sell Some1, unchanged symbol
handling and result string1. Then done_context in the same callback. No HTTP
attempt exists, so do not use an HTTP delivery validator. Admission/duplicate
refusal leaves Store unchanged. Keep setters and their successful no-op policy,
including accountMarginType capability, completely untouched.

Missing credentials keep native text and gain InvalidInput/NotSent. Unsupported
Spot public trades and futures leverage keep text and gain Unsupported/NotSent.
Keyed missing-fill-symbol/order-history/income still return exactly[] without a
history warning; cached hits keep full exact native result; native absence of
companions remains. Shared admission errors keep engine kind/reason and no Read
financial outcome. Keyless query credential failure still precedes kind, cache
and parameter checks. Preserve native remote request/codec/order/filter/range/
limit behavior, primary/secondary identities, signatures, timers and paging.

## Original REDs and compatibility

Commit tests separately, execute frozen production before edits and retain the
same assertions after adoption. Use real Engine callbacks with wrappers that
delegate native guard policies. Seed native market/order/setting rows; no fake
Context/HTTP or private credential authority. A prior no-ID callback allocates
all256 real Background roots and asserts saturation; repeatedly refuse every
reachable local path, retain peers, explicitly retire roots and recover at the
same clock. Invalid raw clocks and fixed-clock reuse cover all eight expressions.

Original REDs pin typed local metadata, shared capacity/time refusal and leverage
projection denied before Store mutation. Compatibility controls pin all native
messages/precedence, cache found results by both client and native ID, sentinel
IDs/missing-symbol cache misses, empty result branches, constant1 setting fields,
successful setting no-ops/capabilities and unmodified private/pager dispatch.
Keep a genuine live command owner to prove duplicate commands cannot mutate Store
or steal completion; finish the original owner and prove same-ID recovery. Private
fallthrough keeps its native signed headers/URL and no newly issued Context or
managed Prepare; existing suites still prove successful remote completion.

## Speed, reviews and final gates

No WS message, ticker/book reducer, Store lookup, discovery or steady-event work
moves. All new work is in cold command callbacks; no per-message registry sweep,
index, counter or layout workaround is added. Check the diff to pin this scope.
Original REDs run on frozen6614 as requested; final integration follows actual
#1888. Report the fresh nine-pair KCS TS/Rust median ratio after integration. The
overseer clarified that1.20x is a headroom target, not a merge gate; the shared
CI speed gate remains mandatory. Do not infer timing from a native test/profile.

Require independent plan/source-adversarial, caller and full-map CLEAN reviews.
Run focused controls, KuCoin (Spot on/off) plus Binance native tests, strict scoped
Clippy/fmt, bans and both request-context checks. Append CALLERS/AUDITS/QUALITY
and README; prune only the eight exact occurrences. Full current CI/replay/size
and an honest KCS timing receipt are required before an explicit venue slot, fresh separate guard
and head-matched merge. #1888's engine window takes priority; no venue merge ahead
of it. Architecture gate: direct shared authority, no venue-specific cross-cutting
mechanism; no local finalizer, fake credential/HTTP, retry, clock or owner registry.

## Tests-first receipt

Plan bfbe1458072 received three independent CLEAN reviews before tests/source.
Tests-first352e80e6677 plus c12cc3b792b run on unchanged frozen6614 production:
ten assertion REDs and six compatibility GREENs. The three standalone REDs prove
denied leverage mutates9/8/7 to1/1/1 and denied cached lookup publishes the full
order. The duplicate-owner control is GREEN on the baseline because the engine
already rejects duplicate commands; no new duplicate-policy claim is made.

The unchanged sixteen controls pass after adoption. KuCoin Spot-on plus Binance
591 tests/43 targets and KuCoin feature-off162/17 pass; strict scoped Clippy,
formatting, bans and both request-context checks (15 checker controls) pass.
Independent source/adversarial, caller and full-map reviews are CLEAN. Final
actual #1873/#1888 integration and current CI remain required before a train slot.

Actual #1888 c55a6a760443bf1531eb536bfc504a312cee1356 is genuinely integrated.
The same sixteen controls pass on that engine; all native source and test blobs
remain exact. Incoming shared docs precede this appendix once, and only the
eight selected legacy occurrences are removed. Current full CI remains pending.
