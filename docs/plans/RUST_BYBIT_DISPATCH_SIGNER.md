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

## Historical implementation checkpoints (unmerged)

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

Historical owner sequence: actual #1734 Aster profile rollback, #1736 financial,
#1735 background coverage, then this migration and #1739 Kraken financial. Actual #1726 stamp
`e09e34486259f4efb44cc04c84a9eec6f37c5974`, #1727 history
`361a4ccafadcf0385d0f708d124596601da83402`, #1729 financial
`6e10b0e8a23dbb8d34f761e8199a83c6068fadb1` and #1731 are integrated.
Actual #1734 `b95cad5a1003d57cdc89bfa117c92b47d3a4c61f`, #1736
`288155e8d5be4e63ebcc77f4916514dd14e01df5` and #1735
`b1dfdb43291d5a6de7dd8808b486adddda4cb118` are now real-integrated.
All final CI/replay evidence must use this actual dependency tree.
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

Integration checkpoint: actual landing through #1735 is merged by porcelain.
Generic provider gates are now unchanged from landing. Conflicts involved two
feature comments and appended host tests; both intents are retained. The shared
negative injection/public-host controls move from newly migrated Bybit to
unmigrated Bitunix. The two existing actual-WASM SDK rejection fixtures do the
same, and a positive Bybit primary/secondary identity-read case verifies exact
headers, MAC and venue userID/inviterID mapping.

Historical pre-extraction validation was 1,025 focused engine/Binance/Bybit/
registry/host tests plus strict checks; post-seam validation was 1,035 focused
native tests, 16 SDK tests, SDK typecheck/lint and Rust strict Clippy/fmt/bans. Final integration tests, actual-WASM CI
and authenticated replay proof are pending; no final signature-equality claim.


## Hosted bridge integration checkpoint (unmerged)

Actual signing-side bridge #1745
`5743c485d44e348e3d29039c4ebe83f6a78f0186` is real-integrated by porcelain
merge `57491e9fba0`. Independent preservation review confirms the exact49-path
prior scope and every incoming setup/capture/build change. The two conflicts
retain all #1738 MEXC/passphrase tests with the unmigrated negative correctly
scoped to Bitunix, and retain shared hosted-work ratchet descriptors while
retiring obsolete Bybit credential-custody entries.

Native HTTP and WS preparation now reuse the landed Bybit hosted work builders;
HTTP bytes, private credential interpolation and clamped window are unchanged.
Native WS explicitly retains its generated expiry receipt; the separate hosted
API continues signing the original caller preimage. Independent review confirms
result application, freshness, ownership/error semantics and the cfg(test) oracle
are unchanged. There is no new executor, retry ledger or raw-secret fallback.

The obsolete closed-admission WASM fixture is replaced with mandatory positive
provider HTTP and WS cases for all five aliases. They use independent Node HMAC
expectations for indexed GET queries, exact JSON bodies and all three WS output
encodings, with primary identity despite secondary setup, original caller expiry,
configured receive window/site routing, custody and zero venue/startup I/O.
Hyro origins are explicitly configured and restored. The generic unmigrated
control moves to Bitunix while retaining its positive legacy WS signature.
Existing Binance positives and the Bybit native primary/secondary read case stay.
These new actual-WASM cases await a fresh CI rebuild; local unit mocks are not
evidence that they pass.

Ratchet classification: #1745's raw textual baseline grew509->520 only through
seven opaque work-builder/import descriptors and four cfg(test) algorithm/fixture
matches in Binance sign.rs and Bybit hosted_signing.rs. Independent inspection
found no new production secret access, holder or cryptographic executor. This
Bybit update reuses those builders and removes obsolete custody descriptors;
its complete raw baseline is517, below landed520, after removing two inherited
unused WOO account allowances at the overseer's request. The baseline equals the
actual scan for every path/hash/count, leaving no allowance for either retired
site. No new fingerprint is added by helper reuse; checker patterns are unchanged.

Focused local checks pass:1,080 native engine/Binance/Bybit/registry/host tests,
86 SDK tests plus five rule tests, SDK typecheck/lint, Rust strict Clippy/fmt and
bans. Standalone Bybit registry and host feature checks also pass; their
focused executions are recorded separately from the default-feature count.
Current merge hold belongs to #1746's global WASM build-policy proof;
this branch must integrate its actual pin before final CI/replay proof. Queue:
#1746, then this #1728, then the other ready venue/history work. Earlier923 CI
and replay receipts remain historical; fresh source-specific proof is required.
Keyless RemoteSigner/PayloadSigner header/feed adaptation remains the shared
bridge's mandatory half2, not a claim of this family migration.
