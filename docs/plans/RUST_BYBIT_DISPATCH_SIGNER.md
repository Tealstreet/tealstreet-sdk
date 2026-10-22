# Bybit dispatch signer adoption

Plan, 2026-10-22. Fresh landing base: actual signer prerequisites #1721
`802e9c8c6e947aa36864aabb7254aa6e50c92b4e`. Covers Bybit linear,
inverse, spot, demo and HyroTrader. No cumulative-fill or read-scheduler changes.

## Implementation

1. Replace venue-held key/secret/read credentials with engine opaque identities.
   Adopt the existing SignStep dispatch path for ordinary REST, the opposite-host
   environment probe and static private WS authentication (no transient token).
   Preserve GET secondary selection, POST/WS primary selection and request routes.
2. Canonical preparation uses shared DispatchContext.stamp_ms(). REST signs exact
   integer protocol timestamp + private credential interpolation + clamped receive window
   + existing ordered query/body, using HMAC-SHA256 lowercase hex. WS signs
   GET/realtime plus shared integer protocol stamp + 10000, preserving string expiry and
   Hyro req_id. Declare actual stamp/expiry receipts without changing wire bytes.
   Preserve immutable client IDs, host-clock timers, one eligible rejection resend,
   environment diagnostics and every existing financial/currentness fence.
3. Extend existing trusted construction/provider gates for the five aliases in
   registry, standalone/combined WASM/Workers/JS hosts and shared construction JSON;
   native compatibility consumes raw setup outside venue state, just as Binance.
   Do not add a second signing executor or expose secrets through generic options.
   Update scoped caller/bootstrap documentation and signing-site ratchet entries.
4. Regressions prove byte identity against the pre-migration oracle for primary,
   secondary, escaped credentials, receive windows and WS/Hyro correlation. Exercise
   delayed provider, stale preparation after calibration, provider refusal/error,
   retired socket and actual release stamps; preserve ordinary ambiguity/no replay.

## Gates and evidence

Architecture gate: this adopts one existing engine choke point, not a copied
per-venue signing service. Any missing common contract blocks implementation at
that seam and is reported to the signer owner. Root owns shared construction/host
integration; bounded venue implementation may be independently reviewed in parallel.

Focused Bybit plus Binance/engine/registry/host tests and strict checks run locally
with four Cargo jobs. CI owns release builds, size/timing and all five Bybit replay
aliases; compare complete raw outputs/signatures against an authenticated actual
landing baseline, excluding only root metrics. Independent correctness/adversarial
review, green CI and a fresh separate exact-scope trial guard precede merge.
Family source may proceed after actual #1721; later RequestContext adoption remains
separate and must preserve raw lease/deadline ownership.

## Implementation checkpoint (unmerged)

The five-alias venue uses only opaque identities; HTTP/probe and static WS recipes
stage through the existing engine dispatch. Original signing code remains a
cfg(test) oracle, with the signing-site ratchet now scanning this sign module too.
The generic provider feature seam was extracted and landed independently in
#1731 at `06b321192a65562db61489b2cbcdcfe4fce5d055`. This PR only opts
Bybit into that landed feature and its construction policy; it adds no other
provider bootstrap or execution path. Typed signer failures retain owned NotSent outcomes and
prior effective/ambiguous aggregate evidence; history keeps its existing finalizer.

347 Bybit native tests pass, including thirteen dispatch/provider regression groups;
standalone registry two tests and standalone host two tests pass. The latter cover
all five setup constructors, while ordinary immediate startup GET checks cover
four aliases: Hyro intentionally waits for its catalogue before private reads.
Its supported owned-read and static WS paths are exercised by venue regressions.
Focused SDK 18 tests, typecheck, lint, strict Bybit/standalone registry Clippy pass.
Independent source review and CI/replay evidence are still required.

Current owner sequence: actual #1731 provider seam, then #1734 Aster profile
rollback, then this migration. Actual #1726 stamp
`e09e34486259f4efb44cc04c84a9eec6f37c5974`, #1727 history
`361a4ccafadcf0385d0f708d124596601da83402`, #1729 financial
`6e10b0e8a23dbb8d34f761e8199a83c6068fadb1` and #1731 are integrated.
#1734 is pending and must be integrated before final CI/proof.
RequestContext remains a later shared fan-out, not a gate on this Bybit
migration; the existing legacy bridge stays explicit until that adoption.

Command outcome aggregation remains an explicit RequestContext consolidation
target: Binance's reached_venue guard and Bybit's refused/effective/unknown flags
currently live in their adapters. This migration must preserve existing evidence
independently of first-error text; it introduces no additional aggregation service.

Independent adversarial review also reproduced mixed provider/native failures in
both orders: an actual definitive native refusal must update existing aggregate
evidence even after an earlier provider error. Bare HTTP400/429 cases retire the
provider-only classification and retain the existing native classification.
Both compiled RED regressions now pass with no new aggregation state.

Four additional ordinary/Hyro retry cases reproduced provider-only NotSent after
an earlier native signature rejection or transport callback. Owner-approved
conservative behavior: Rest.resent routes retry-provider failure through the
existing sanitized failure/reconciliation path, yielding Unknown; no second
mutation is sent and planned identity/body stay immutable. Precise native
Rejected versus transport Unknown provenance remains RequestContext work.
These cases compiled and failed before the change; all thirteen groups pass now.

Integration checkpoint: actual landing through #1731 is merged by porcelain.
Generic provider gates are now unchanged from landing. Conflicts involved two
feature comments and appended host tests; both intents are retained. The shared
negative injection/public-host controls move from newly migrated Bybit to
unmigrated Bitunix. The two existing actual-WASM SDK rejection fixtures do the
same, and a positive Bybit primary/secondary identity-read case verifies exact
headers, MAC and venue userID/inviterID mapping.

Historical pre-extraction validation was 1,025 focused engine/Binance/Bybit/
registry/host tests plus strict checks. Final integration tests, actual-WASM CI
and authenticated replay proof are pending; no final signature-equality claim.
