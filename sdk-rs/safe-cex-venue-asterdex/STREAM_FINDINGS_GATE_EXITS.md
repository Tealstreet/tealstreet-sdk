# Asterdex stream effects evidence

Latest measured production SHA: `8d8daf924efa0b55a9c01eec8ca139e8642a2262`. Scoped all-target check:0;
public/private stream target:0,15 tests. The five public null class/message
assertions compare actual-source errors before normal native Store checks.
Concrete account null entry now throws TypeError before emission; the focused
account helper/effect-order tests pass0 (two tests, source reorder child101).
Cumulative helper inputs are byte-identical to d54baab36e, whose two focused
live-source tests passed0. Null-path repair was the second review blocker;
manifest CHOSEN notes now name why retaining untyped referents would violate
this handler's native fields and why owned callbacks release Store borrows.


## Reviewed repair

Measured repair SHA: `d54baab36e195459ab7067374c278d008833c9ba`. Scoped all-target cargo check:0.
The live_public_stream +live_private_stream target:0,14 tests. Focused
`live_account_helpers mirror_cumulative_order_fill`:0,2 tests,14 actual-source
fill cases(last13), including a source field-swap child that fails101.
Native missing-ID coercion tests cover two failing quantity slots(last1); the
clock is asserted lazy. The test first failed101 on b729548938 because an
identical adjacent helper guard was targeted; d54baab36e restores that adjacent
guard and moves the cumulative guard only after both quantity parses.

A fresh write-only account mutation at this repair SHA (`false && B` guard)
failed101 at `private dispatch message-routing step8 total`:native0/source10.
The restored14-test stream target passed0. The earlier account-selector survivor
is retained below. No source or manifest change was made by these temporary
controls. The five renamed public handlers and extracted private handler keep
the same seven-row census reported below.

## Earlier measured snapshot

Measured source/test SHA: `84cf845938aec9d5e4f3d387273a69b8f659b6d9`. Fresh parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
No unmerged feature branch is an ancestor.

Seven FINDING rows now name concrete Store/callback bodies. Two divergent rows
move to ported; five retain their measured CHOSEN typed/owned boundaries, citing
[the canonical declarations](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary)
and [alias declaration](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).
Tomllib census: 130/92/9 ->132/90/9 (ported/divergent/pending); zero pending rows
moved. FINDING-containing notes:30 ->23. This census is not an equivalence proof.

## Measured exits

- Scoped `cargo check -p safe-cex-venue-asterdex --all-targets --jobs 2`:0 at
  fbcedf14fe; Rust source/test inputs are unchanged in 84cf845938aec9d5e4f3d387273a69b8f659b6d9.
- `cargo test -p safe-cex-venue-asterdex --test live_public_stream --test live_private_stream --jobs 2 -- --include-ignored`:0 at 84cf845938aec9d5e4f3d387273a69b8f659b6d9;13 tests, Node24.20.0.
  The independent scenario floor is16 public (last index15),25 private (last24).
  Decoded native fields/Store state precede wire checks.
- Extractor writer:0, Updated0 manifests,0 added/orphaned. Extractor check:0,
  Would update0 manifests,0 added/orphaned, at fbcedf14fe; TS and manifest inputs
  are unchanged in 84cf845938aec9d5e4f3d387273a69b8f659b6d9.
- Rustfmt on the two touched modules and native fixture:0.

## Discriminating controls

Temporary compiled handler omissions each failed101: ticker(last), mark(mark),
trade(last), depth(book callback count), kline(candle callback count),
mirrorCumulativeOrderFill(native cumulative fill before emission). These ran
against the 5bd9c52e7a source/test snapshot. A separately compiled omitted account
handler failed101 at private dispatch message-routing step8 currency presence.
A source ticker last/percentage swap failed101 at native ticker last (13 vs2).
All temporary source/Rust changes were restored; the13-test target then passed0
at the measured SHA above.

SURVIVOR: selecting private_account_matches_source for handleAccountUpdate
omission initially passed0. That test covers ACCOUNT_UPDATE rather than the
outboundAccountPosition call site. The maintained fixture now supplies a real
USDT balance update on the latter path, checks independent10/8/2 native fields,
and captures the balance inside the actual source/native account emitters.
The corrected native dispatch omission control failed101. The survivor remains
recorded here; it is not counted as a passing mutation gate.

An initial restored run of the new independent balance assertion failed101
because the raw fixture still held B:[]; the raw ingress was corrected in
84cf845938 and the restored target passed0. No failing run is reported as green.

## Not run

Workspace/full-crate/monorepo suites, clippy, wasm, Node26, live account/API calls,
load tests and CI reruns. No exhaustive domain claim follows from these floors.

Final extractor check at52768a8722:0, Would update0 manifests,0 added/orphaned.
The writer immediately before that commit also exited0 with Updated0 manifests.
Final readonly sub-agent decision:MERGE for production8d8daf924e and the reviewed
note-only changes. Source/test/script/Cargo inputs compare byte-identical from
8d8daf924e to52768a8722 (git diff exit0). No additional gates are claimed.
