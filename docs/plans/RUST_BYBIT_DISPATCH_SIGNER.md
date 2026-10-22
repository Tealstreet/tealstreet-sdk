# Bybit dispatch signer adoption

Plan, 2026-10-22. Fresh landing base: actual signer prerequisites #1721
`802e9c8c6e947aa36864aabb7254aa6e50c92b4e`. Covers Bybit linear,
inverse, spot, demo and HyroTrader. No cumulative-fill or read-scheduler changes.

## Implementation

1. Replace venue-held key/secret/read credentials with engine opaque identities.
   Adopt the existing SignStep dispatch path for ordinary REST, the opposite-host
   environment probe and static private WS authentication (no transient token).
   Preserve GET secondary selection, POST/WS primary selection and request routes.
2. Canonical preparation reads DispatchContext.now_ms only. REST signs exact
   floored timestamp + private credential interpolation + clamped receive window
   + existing ordered query/body, using HMAC-SHA256 lowercase hex. WS signs
   GET/realtime plus floor(protocol time + 10000), preserving string expiry and
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
