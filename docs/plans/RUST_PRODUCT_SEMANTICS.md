# Phase 9 product semantics

Base: `a7caec9314` from `origin/feat/rust-rewrite`. Branch:
`feat/rust-product-semantics`. This work retains every TS venue and keeps
Rust opt-in. Follow `VENUE_PORT_BRIEF.md`; full-auto execution is authorized.

## Committable phases

1. Carry typed instrument classification through an optional indexed frame
   section. Preserve absent versus empty native labels, market region and
   underlying ticker. Reconstruct only documented semantic fields. Verify real
   Rust discovery, transferred frames and `prepareCliOrder` TradFi refusal,
   including stock, ETF, commodity, FX, crypto, innovation and unknown labels.
2. Implement Bybit combined linear/inverse/options categories inside its
   existing crate. Forward saved flags, preserve category-specific feeds,
   account reconciliation, routing, quantities, option terms and Greeks. Test
   each enabled combination and existing standalone profiles.
3. Extend OKX's existing crate to native inverse swaps and EU xperp futures,
   preserving account/host/demo routing, native quantities, quote valuation,
   subscriptions, commands and histories. Test product and consumer sizing.
4. Resolve GRVT Future/Call/Put catalog and product semantics explicitly.
   Retain supported native orderbook products with typed identity and correct
   signing/account/query/cancel behavior. Do not infer RFQ support or a Sam
   retirement decision. Document any separately unsupported product precisely.
5. Run independent adversarial phase reviews and repair findings. Run bans,
   format, strict Clippy, native/host/frame/CLI tests, root typecheck/lint/tests,
   fresh public/private replay gates for touched venues and Binance profiles.
   Record correctness, request coverage, CPU, retained memory and artifact size.
6. Update phase-9 readiness rows; push a PR into `feat/rust-rewrite`, publish
   correctness reviews, fix and repeat. Merge only after green CI on the final
   head, then cut a fresh branch from the landing branch.

## Audit decisions

- Optional indexed semantic sections keep ordinary market columns unchanged;
  replacing markets invalidates metadata and a metadata change emits a market
  replacement. Native labels are retained rather than guessed into crypto.
- Venue agents own disjoint crates; the primary agent owns shared frame,
  wrapper and consumer integration. Artifact builds and replay gates have one
  owner to prevent stale or mismatched generated bindings.
- An inverse-only alias does not prove combined-account parity. A perpetual
  fixture does not prove dated futures/options, and synthetic private traffic
  does not establish live connectivity.

## Evidence

Implementation, phase audits and gate results are appended at stable commits.

### Implementation checkpoint

- WIP checkpoint `a3f9cee4a5` preserved the implementation before merging
  landing `0063068241` in `a7ce59126b`; no stash or rebase was used.
- Shared engine tests: 74 PASS. Existing Binance native control: 99 PASS.
  Bybit: 137 PASS (123 existing plus 14 product regressions), strict Clippy
  PASS. GRVT: 93 PASS, strict Clippy PASS. OKX: 77 PASS before the live
  optional-cap regression described below. The combined native matrix passes.
- Root typecheck: 44 tasks PASS; lint: 28 tasks PASS; full repository tests:
  27 tasks PASS, including 505 safe-cex files / 7,329 tests. Focused current
  wrapper/inverse consumer regressions: 20 PASS; actual CLI policy tests: 22 PASS.
- Independent phase reviewers found and closed: wrong inverse native-PnL
  labeling in the shared consumer; incomplete EU history joins; Bybit global
  history limits hiding later categories, sibling settings replacement,
  reconnect healing only one category, malformed category snapshots erasing
  state, IV bypass on non-option protective branches and cold inverse mark
  fallback. These are implementation-review findings, not all TS bugs.
- The classification integration fails against the previous artifact because
  the native semantic section is absent. With the new artifact it passes real
  Bybit discovery, worker frame application, actual buffer transfer, main frame
  application and `prepareCliOrder`: stock/ETF/commodity/FX refuse, empty and
  innovation crypto pass, absent and future labels remain unknown.
- First fresh 20-second public captures: Bybit 17,591 frames / 912 markets,
  GRVT 4,412 frames. OKX/EEA captures expose empty optional order-type caps on
  inactive preopen rows; native parsing and the independent TS reference are
  being corrected before the public gate is accepted.
- Final native/combined WASM, public/private replay, artifact-size, adversarial
  PR review and exact-head CI evidence are still required. Live private
  credentials, Hyro origins and combined options connectivity are not claimed.
