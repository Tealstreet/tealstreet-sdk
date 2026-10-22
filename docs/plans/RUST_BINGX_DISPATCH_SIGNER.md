# BingX native dispatch signer checkpoint

Plan, 2026-10-22. Fresh actual base
`5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6`. #1718 inventory is retained in
`RUST_SIGNER_HMAC_MIGRATIONS.md` on its separate plan branch. No OKX/Bitget or
unmerged half-2 source is inherited. Keep #1737/#1757 watches active.

WOO was checked first and deferred without edits: its write gaps are recorded at
staging, so parked-provider + fast-ACK physical pacing belongs to RequestContext.
BingX has no equivalent write-gap/expiry dependency and uses HMAC-SHA256 hex,
without passphrases. Native HTTP adoption can proceed on the landed signer API.

1. Replace raw Options and sign::Credentials with opaque primary/read Identity
   references. Trusted shared setup owns all material; registry/WASM BingX feature
   gates select dispatch signing without Binance. Incomplete primary retains
   public construction; malformed secondary remains an early rejection. Keep
   native user/account IDs, no key-derived identity or new stable-ID fallback.
2. Keep Params/BTreeMap native typed serialization: canonical RFC3986 sorted
   query including shared integer protocol timestamp, HMAC-SHA256 hex signature;
   final query re-sorts signature normally. POST sends exact typed JSON (numbers,
   booleans and u64 IDs stay native), other verbs query/no body. X-SOURCE-KEY,
   X-BX-APIKEY and Content-Type header order remain exact. Public quote work stays
   unsigned. Listen-key HTTP is atomic Present-only, no timestamp/signature.
3. Real issue/resend and independent fetch_identity builders stage SignStep with
   immutable Params recipe and selected refs. Ordinary GET selects secondary;
   writes/listen POST/PUT select primary. Preserve account read scope/slot tags,
   streamed ownership, primary/secondary egress and existing signed wrapper.
   Same-callback reprepare uses protocol time; raw timers/deadlines remain raw.
4. Preserve one eligible100001 signature resend (including streamed rejection).
   Provider failures do not invoke it. Typed refusal unwraps the existing pending
   owner: identity/history strict failure; trade first unsent notSent, prior
   resent attempt Unknown, existing accepted effects retained. Read-owned
   metadata failure cannot regress newer rails or become native auth rejection.
   Background account cleanup retains existing generation/recovery checks; no
   new retry, outcome ledger, calibration or scheduler.
5. Existing listen-key acquisition/renewal and socket subscription/reconnect
   lifecycle stay unchanged. No static WS HMAC exists and no new token custody
   path is added; the owner WS-auth core owns later session standardization.
   Present-only work uses the shared provider; no venue key read or raw restoration.
6. Preserve old exact vectors as cfg(test) oracles, migrate native fixtures to
   trusted provider setup, and add actual-builder refusal/late/delayed/reprepare,
   POST type/body/client-ID, slot/primary listen presentation, metadata and resend
   controls. Include runtime RED owner-routing regressions and preserve all
   financial/history/socket assertions. Shrink exact ratchet and scan sign modules;
   pure work descriptors/test oracles do not authorize runtime custody/crypto.
7. Include same-PR caller and BingX CLAUDE notes, worker/provider bootstrap and
   draft exposure boundaries. This native checkpoint does not claim hosted
   signHttpRequest/keyless admission; standalone request shaping/policy/actual-WASM
   positives follow actual half-2. No encoding/passphrase extension is required.
8. Focused BingX+Binance/engine/registry native gates, strict fmt/Clippy, SDK setup
   type/lint checks, independent correctness/adversarial audits. Final actual
   dependency integration/full CI/authenticated replay and own-only guard precede
   merge. No local release builds/full replay/size/timing runs.

Architecture gate: existing engine SignStep/provider/setup adoption. No new
per-venue cross-cutting concern; WOO pacing was reported rather than patched.
Settings/store freshness, row admission, financial execution and metadata owner
work remain intact and coordinated with their lanes.

## Native recipe checkpoint

The additive `sign::dispatch::Http` recipe stages opaque provider refs and uses
the existing typed renderer. Production builders remain on their old path until
the next checkpoint; this commit does not claim custody migration or readiness.
The five dispatch tests cover native method/body parity, Present-only listen
requests, changed-intent rejection before provider work, malformed provider
results, and stale-result reprepare with old-attempt suppression. BingX/Binance
native gates and strict checks pass. Independent correctness and adversarial
source reviews found no blocker; the latter's missing negative controls were
added before committing.

Owner approved the existing `job.effective/error/ids` migration adapter: a prior
sent leg must never become wholly NotSent; uncertainty stays Unknown. Add RED
cases without new state flags or ledgers, and identify the BingX finalizer as a
RequestContext consolidation target in the PR body. Account signer cleanup must
compare the existing in-flight revision before clearing a latch. The separate
unmerged metadata core is not inherited, and its publication fences stay intact.

## Native staging checkpoint (draft)

Real issue/resend and independent identity builders now stage the shared recipe;
venue Options/Credentials contain opaque refs only. Registry and standalone WASM
feature gates select trusted setup. Existing native tests use provider setup,
with the old full signing implementation retained exclusively as a test oracle.
All BingX native tests and the BingX-only registry build pass; strict gates cover
the touched crate and registry. Independent reviews preserve success bytes,
secondary GET/primary writes, streamed ownership, resend tags and existing
metadata fences. Review caught and restored the valid-primary fixture in the
invalid-secondary/no-fallback test.

This is an intermediate checkpoint, not PR readiness: typed owner failure routing,
existing-state conservative outcome RED controls, revision-fenced cleanup, caller
section, exact ratchet shrink and hosted/CI proof remain. Work pauses here for
the owner's #1757 final integration priority after actual #1737.
