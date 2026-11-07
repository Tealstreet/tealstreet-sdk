# Bitunix ordinary order reconciliation under WebSocket traffic

Follow the scheduling prerequisite with independent keyed admission. Ordinary
regular/TP-SL snapshots use current endpoint serial plus an order command revision.
Command-owned prerequisites keep their existing global generation/serial checks.
Capture bounded overlap footprints per active read and staged initial half:
validated newer raw families, and separately malformed known/unknown identities.
Transfer footprints before prerequisite success is computed. Preserve complete
current raw families/native metadata and terminal absence over older REST rows;
healthy REST peers and sparse unknown execution metadata repairs must progress.
Never consume a baseline for a superseded or quarantined family.

Track mutation revisions at optimistic placement/cancel, write dispatch, accepted
write responses (including partial cancel ACK before an error exit), and job cleanup/error.
Preserve positions settings revision, native outcomes and no-write-retry policy.
Current malformed WS coverage fences ordinary affected families/absence without
stalling every peer. Bounds at 4096 hold conservatively and diagnose once.

Initial halves keep receiving footprints after HTTP completion. Both complete,
current serial/revision halves with no malformed coverage are required to publish;
valid newer WS overlays may complete startup, malformed coverage may not.

Regress both endpoints, newer family metadata, terminal absence, malformed known
and unknown coverage, continuous malformed notifications, sparse unknown fill
repair, baseline preservation, staged initial halves, strict owned preflight,
optimistic/ACK/partial-ACK/cleanup races, late callbacks and bounded coverage.
Prove meaningful original red controls, run Bitunix/Binance and strict gates,
independent reviews, fresh full CI and separate own-only guard. Genuine dependency
on scheduling + clock/login/routing stack only; no unlanded sibling commits at merge.

Implementation review found an initial newer-footprint overflow authority bug;
`partial_orders` now fences active halves, staged pairs and prerequisite success.
The four active/staged × endpoint overflow cases fail when this guard is disabled.
Thirteen regression groups pass; eleven fail against the original scheduling-only
source, including the pre-existing partial-cancel ACK reopening regression.
Bitunix/Binance native tests (440) and strict Clippy/format/bans pass. CI, final review and
actual predecessor integration/guard are still required before merge.

Owned read publication also requires the write revision, independently of the
preflight result. Its dedicated partial-ACK race fails with only that publication
guard disabled (Canceled becomes Open) and passes with the guard restored.
