# Paradex independent startup reads

Base: `fa5d5fb90c4` on `feat/rust-rewrite`. Lane: codex-zs5e83.
Full 62-occurrence/53-fingerprint inventory and destinations are committed in
`f4301a83317:docs/plans/RUST_PARADEX_REQUEST_CONTEXT.md` (draft #1813).
This independent slice changes only the shared public startup send site;
#1813's book/lib files and #1787's account/trading files remain held.

## Plan

1. Add assertion-RED controls in `tests/request_context_seeds.rs` using the real
   venue and explicit Prepare/Bind/Claim. Check live/testnet routes, empty GET
   body/headers, no early publication, native streaming, malformed prefix
   rejection, preparation denial, original 20-second deadline, stale response
   suppression, and recovery through the existing engine startup Backoff.
2. Add a `ManagedSeed(Context, Box<Request>)` request variant in `src/public.rs`.
   `/markets` and `/markets/summary?market=ALL` each issue an independent public
   background Read Context (20 seconds, RetryPolicy::Never, no credential).
   Use existing managed ingress, streaming and venue-owned item parsing.
   Validate exact delivery while parsing/publishing; retire on every success
   and failure before native early returns or Backoff processing. Keep the
   native catalog/ticker codecs, LiveTickers and private-start independence.
   These are separate startup tasks, not one multi-request command Plan.
3. Adapt only unheld fixture drivers `tests/support/mod.rs` and
   `tests/startup_resilience.rs` to the shared prepared_actions handshake.
   Preserve timers, response ordering, payloads and all existing assertions.
4. Prune only the vanished public_seed request_streamed fingerprint. Baseline
   public.rs has 8 occurrences; expected 7 afterwards (venue total 62 -> 61).
   Candle fallback/polling, accounts, history and private auth remain on their
   existing paths and keep their exact fingerprints.
5. Run focused Paradex + Binance suites, strict Paradex Clippy/fmt, bans and
   both request-context checks. Obtain source/adversarial, caller and full-map
   independent reviews; append landed caller behavior and proof records.

## Caller effects

Custom transports prepare and claim the startup GETs. Preparation and physical
HTTP share the original 20-second attempt bound. Failure retires the attempt,
then existing startup Backoff schedules a fresh independent attempt. No command
result is emitted. Market/ticker shapes, partial valid-row filtering, account
initialization after catalog publication, live ticker dominance and public feed
policy behavior remain governed by their existing native logic.

## Architecture audit

No per-venue cross-cutting mechanism is introduced: Context issuance, ingress,
release, deadline and Backoff are existing engine paths. The wrapper carries
native staging data and authority only. No engine changes or local retries.

Plan audit: independent source/caller/full-map exploration found no held source
or test-file collision; the unheld startup_resilience fixture also needs the
shared handshake. All fixture timers must remain visible. Source adoption must
handle refused issuance and managed queue errors without command-ID fallback.

## Implementation checkpoint

`d8e8c5f9e91` compiles and all four controls fail against the original source at
legacy send assertions. The implementation removes only fingerprint
`daa9160114df450bc32e689954faee7fd2d88713b62477953faf17c6ac8e3ebf`.
Source/adversarial, caller and full-map independent reviews are CLEAN; the latter
also verified fixture token identity apart from the shared handshake. Strict
all-target Paradex Clippy, scoped fmt, bans, ratchet and 13 checker controls pass.
Complete Paradex + Binance suites and CI are required before ready/merge.
A test assertion was corrected to permit managed Cancel cleanup while continuing
to reject any new Prepare/Bind/HTTP before startup Backoff fires.

Full-suite follow-up: all Binance suites passed (293 tests / 24 suites). The
Paradex price-band unit fixture inside `public.rs` also selected only legacy
HTTP. Its two drains now use the same shared preparation helper; original
payloads, price-band normalization and valid-row assertions remain unchanged.
Complete Paradex verification continues on the corrected fixture.

Final local proof: Paradex 192 tests / 13 suites and Binance 293 tests / 24
suites pass (485 tests total), including all four new lifecycle controls.
Strict all-target Paradex Clippy and scoped fmt pass after the unit-fixture
correction; both request-context checks still pass. CI remains the merge gate.
