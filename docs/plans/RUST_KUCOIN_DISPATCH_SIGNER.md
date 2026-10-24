# KuCoin futures and Spot opaque dispatch signer cutover

Owner: codex-9czdjh. HMAC/native reviewer: claude-4yjlw0. Credential/role reviewer:
claude-m26ps4. Base: actual `77f12408947`; branch `feat/rust-rc-next-slice`.
This refresh replaces the historical ba3d/56-site plan. Plan and original REDs
only until both reviewer scopes are CLEAR; no source or inventory transition is
approved by this document.

## Scope and current accounting

The exact checker inventory is **46 occurrences / 42 path-local fingerprints**.
All 46 are listed below. The signer/broker cutover removes their custody gate;
it does not itself prove every history, execution, session or Mutation owner has
migrated. Conservative test/macro rows remain counted until real call-site
conversion makes them obsolete; no renaming or rewritten assertion is a prune.

| Path under sdk-rs/venues/kucoin/src | Occurrences | Fingerprints | Work unlocked after signing cutover |
| --- | ---: | ---: | --- |
| account.rs | 3 | 3 | Secondary account/config/position/order reads and their original owner lifetimes |
| funding.rs | 1 | 1 | Selected funding pages, 400005 scheduling and original history owner |
| history.rs | 5 | 5 | Fill/order/income pages and authenticated public-trade reads |
| identity.rs | 2 | 2 | Futures FetchAccount genuine selected root and its two terminal sites |
| lib.rs | 7 | 5 | Four physical issuers, conditional Primary public signing, candle terminals |
| secondary_tests.rs | 10 | 9 | Existing key/passphrase/public-only/broker assertions; retain semantics |
| spot/mod.rs | 11 | 10 | Private requests, identity, lookups, fills and token setup; public discovery/candles stay managed unsigned |
| spot/trading.rs | 4 | 4 | Sequenced Spot writes and accepted-ID terminal aggregation |
| trading.rs | 3 | 3 | Futures writes/mode preflight and retained accepted IDs |

#1986, #2017 and #2019 are actual. #2022/#2024/#2029 are unrelated frozen
candidates and must remain untouched. The current open-PR file map has no KuCoin
production collision. Shared registry/crypto/setup files have concurrent owners;
production requires an actual integration or an explicit narrow composition
window, never wholesale adoption of an old branch tree.

## Existing broker work: provenance and reuse boundary

`62377e9368e` is m26's latest broker checkpoint, not an actual landing or frozen
standalone PR. Its parent composition contains unlanded #1973 `b89c2f39b9d` and
#1990 `fd531f4b01c`; `fa9d7592b7c` composed their sign_key paths. Reuse the reviewed
broker deltas `d545ee95bb4`, `62e412720d3`, `62377e9368e` after those prerequisites
land or a maintainer authorizes exact composition. Do not copy their 27-path
aggregate diff onto the current tip or rewrite their shared algorithm resolver.

Broker responsibilities are one declared role and generic setup policy:

- Add `ExtraKey::KucoinBroker`, wire name `kucoinBroker`, bound to HmacSha256 in
  the closed role table supplied by #1973/#1990. A broker ref rejects every other
  algorithm, Present and Credential interpolation. Ordinary HMAC refs keep their
  ordinary behavior; do not claim that HMAC signing alone proves a broker role.
- Primary extra policy is Optional UTF-8 with native Rust str::trim. Empty or
  blank-after-trim input mints no ref, even if an input encoding would otherwise
  fail. U+FEFF remains in the material, matching Rust Unicode White_Space.
- Secondary extra policy is AliasPrimary: exactly the primary's one existing
  role ref, or none. Secondary-supplied material is refused; it is not a fallback.
  Trusted setup installs the material once. Recipe lookup uses only the selected
  Identity.extra; a missing alias never authorizes reaching into another slot.
- Setup and generated policy reject AliasPrimary on primary, aliases whose role
  differs from the primary role, and trim/encoding on an alias declaration.
- Partner/name are non-secret metadata, Rust-trimmed. Verify remains Option<bool>,
  default true and explicit false preserved. All material, including the nested
  broker secret, leaves generic options before venue construction; no second raw
  copy or REST fallback. Broker configurations remain supported.

The existing branch's unit checks use internal key-map counts. New isolated
integration REDs on this tip prove public contracts without editing claimed
crypto/unit-test bodies. The known missing JSON role/policy is a NEW-CONTRACT
runtime rejection, not proof that later alias/MAC assertions already executed.
No G2 issuer/subject/config-generation retirement proof is inferred from a
Selection, Identity or broker alias; that remains separately owned shared work.

## Atomic recipe and native bytes

One dispatch receipt stamp, one atomic SignRequest, ordered operations:

1. Present the selected API key ref.
2. HmacSha256(selected secret, stamp + METHOD + rawPathAndQuery + exactBody).
3. HmacSha256(selected secret, Credential(selected passphrase ref)).
4. Only for a complete broker triplet: HmacSha256(selected broker role ref,
   stamp + trimmedPartner + Credential(the same selected API key ref)).

MACs render as standard padded Base64. Never Present the raw passphrase or broker
material. The sign payload uses the raw path/query; transport separately uses
encoded_path(rawPathAndQuery). Preserve order, duplicates, plus signs, Unicode
and exact body bytes. Attest outbound method, allowed origin, encoded URL, body,
ordered unsigned headers and empty signer headers in prepare AND apply. Verify
same receipt, exact result shape and lengths, no CR/LF key/header injection and no
provider-supplied header override before exposing the atomic result.

Headers keep native order: KC-API-KEY, KC-API-TIMESTAMP, KC-API-KEY-VERSION=2,
KC-API-SIGN, KC-API-PASSPHRASE, Content-Type, X-CHANNEL-API-CODE. A complete
partner/name/ref triplet appends KC-API-PARTNER, KC-API-PARTNER-SIGN,
KC-BROKER-NAME, KC-API-PARTNER-VERIFY. Any missing/blank member suppresses all four
and the broker operation. Request and broker MACs share the same receipt stamp;
passphrase MAC has no timestamp in its preimage.

Private GETs select Secondary only with a usable Primary, else Primary. An absent
secondary passphrase inherits the primary's opaque ref; explicit empty refuses.
Writes/DELETE/private bullet use Primary. Futures authenticated public discovery,
depth, candles and public bullet use Primary, even with a read key. Spot public
requests stay unsigned. Supplied read keys validate before public-only/testnet
startup gates. Do not trim key/secret/passphrase bytes beyond trusted native
policy; partner/name/broker alone have native trim semantics here.

Use shared Selection for role/identity/egress/evidence together. Eligible genuine
command reads use SelectedRead/ReadRoot; declaration and original caller receipt
are immutable. Remaining account/coalesced/session/history chains keep explicit
native staging until their owner cutover; no invented 20s logical root for a
multi-attempt, paged or background chain. Each native physical attempt keeps its
20s cap. Scope and pending values survive typed queue/provider refusal. Never
cast a Context/Plan/Slot or reuse a numeric id to manufacture ownership.

## Retries, sessions, outcomes and hosted signing

Every eligible 400005 reissue prepares a fresh atomic request and receipt; never
replay headers. Funding/history retain 1000 * retries ms and native eligibility;
provider failure neither skips nor doubles that delay. Account has its existing
shared retries<1 signature / retries<2 other-failure budget and page resets.
Native 400002 is AuthInvalid, not a clock retry. Spot gains no generic retry.
A two-attempt control must compare changing request/broker MACs and unchanged
passphrase MAC, each genuinely recomputed in its own batch.

Preserve token_pending, generations, dynamic endpoint/token, welcome-before-
subscribe, shared token retirement and heartbeat/supervisor timing. Keep Futures
Direct-only public bullets and independent REST/account startup; Spot's ordering
is different. Delayed provider errors and obsolete bullet replies must not
publish data, wedge flags or revive retired sockets.

A shared KuCoin CodeTable owns native error facts in managed write adoption;
accepted IDs stay structured and journal-owned. No local status ladder, success
fact, NotSent demotion or fabricated ResendGrant. Provider failure before any
mutation is wholly unsent; prior writes/effects retain uncertainty and IDs.
Native legacy journals remain explicit until their own reviewed conversion.

Sam's current hosted policy allows the server the minimum signing material for
each venue. KuCoin requires selected API-key presentation, the selected secret
for the two MACs, passphrase interpolation and an optional broker role ref; send
only operation-needed material through the trusted setup/provider contract. The
old single-signature delegated callback cannot implement this atomic batch by
itself. Its minimum-material and multi-operation bridge needs a shared reviewed
composition; no venue-specific callback materialization or raw REST fallback.
Do not enable hosted admission until the real bridge supports the batch and its
negative controls. This is an implementation gate, not a privacy-policy veto.

## REDs and working controls before source

Reuse the earlier seven-test native corpus from `ab14830a4aa` with its assertion
bytes unchanged: three compiling original provider-custody REDs (Futures
catalogue, Futures identity, Spot identity) plus four working native/provider
controls. Execute it again on this actual tip with spot on/off. The same recording
provider must execute a genuine atomic two-MAC positive control; no failing mock
or compilation error counts as a native RED.

Add broker role/setup NEW-CONTRACT checks using public JSON policy decoding:
role availability; fixed-stamp selected-key broker MAC against independent
Python vectors; native trim/BOM and blank behavior; same-ref alias and supplied
secondary refusal; missing-primary behavior; wrong algorithm, Present and
Credential denial. Pin primary-alias refusal and cross-role/alias-metadata
validation in the shared codegen checks before production. Distinguish the first
runtime schema rejection from downstream assertions not reached yet.

Native broker controls run on unchanged source: full header order/default/false
verify, partial-triplet suppression, selected-read-key broker MAC, raw query and
same timestamp. Add a broker-enabled original provider-custody RED; the raw
native vector must pass before provider wiring. Fresh 400005/provider-deadline/
obsolete-token negative controls remain mandatory before final source freeze.
No ignored or weakened original assertions, no codegen by hand.

## Source sequence and accounting freeze

1. Commit this plan, then separate RED tests and exact execution receipt. Obtain
   4y's HMAC/native CLEAR and m26's role/setup CLEAR before production.
2. Re-integrate actual #1973/#1990 or an explicitly authorized exact shared pin;
   preserve incoming unowned source. Reuse/review broker deltas, then coherent
   family custody cutover: no disabled endpoints, raw fallback or signer-only
   growth. Retain explicitly staged native owner lifetimes where required.
3. Eligible single-command reads and public-only routes adopt existing managed
   entry/headers/unsigned routing and ReadRoot/ReadLeg, validating before Store.
   Staged issuance uses existing shared Selection.sign where truthful; do not
   clone selection, timestamp, outcome or managed-read mechanisms locally.
4. Rescan exact candidate. Obtain an explicit grant for any new STAGING_BOOTSTRAPS
   or SIGNER_POLICY_BOOTSTRAPS maps before editing them. No numeric shrink is
   promised from an unbuilt funnel. Keep frozen call expressions stable for
   later pruning; other families' maps and ordering remain exact.
5. Strict touched-crate checks, Binance controls, feature-off provider controls,
   independent source/caller/full-map review and fresh full CI. Test isolated
   KuCoin and kucoinspot host features, not only unified builds. CI supplies
   WASM sizes/replays; preserve KCS CPU margin. No merge without the assigned
   window, separate revert guard and head-matched merge.

Architecture gate: **No**. Canonical bytes/native continuation belong to the venue;
role binding, setup, custody, Selection, clocks, HTTP authority, retries and
terminal ownership stay in shared choke points. Missing seams remain explicit.

## Exact current inventory (all 46 occurrences)

Path-local fingerprints below come from request-legacy-sites.json at the frozen
base and were independently compared with the current source scanner. This is
an exhaustive unblock map, not a prune grant. Every row stays until actual
source conversion/obsolete-control cleanup proves its removal.

| Path under sdk-rs/ | Fingerprint | Count |
| --- | --- | ---: |
| `venues/kucoin/src/account.rs` | `0763c380b5595f2dd5a739dc559d90a93fad2f0d4ea0c72dd26854c9a87c14e7` | 1 |
| `venues/kucoin/src/account.rs` | `0cb27533973d0ad65fbf7717bc8f06498a1449efee8ca32173d60c0db9ac46b1` | 1 |
| `venues/kucoin/src/account.rs` | `c22f9a403657e792ba5fc1ff078bc6061bfbb345e9724d6ebc11213628568639` | 1 |
| `venues/kucoin/src/funding.rs` | `8c5c5f6e886f77e292bbb10a99aaf8c16333fdd60c7af0fb4880afcc0f040560` | 1 |
| `venues/kucoin/src/history.rs` | `6dc8ff5c9a3f54afd832946af7fc2e777f459ef0ebb8da410821660b9044f481` | 1 |
| `venues/kucoin/src/history.rs` | `76cccead438e68521694435ebb3adb21716e5c650bdd63ae55312345486336e3` | 1 |
| `venues/kucoin/src/history.rs` | `770f2dcac9b528f2bacfc9329e543f5215a004fb671054775207c827e04764a8` | 1 |
| `venues/kucoin/src/history.rs` | `782dad616e9bbc6860101de1c4c5458533faa67d9b51a888ebd1dbfd205aa1ec` | 1 |
| `venues/kucoin/src/history.rs` | `ae699603109cbd1c71356f8a3a96c279698017af0d318744b47d63a2bf08aef4` | 1 |
| `venues/kucoin/src/identity.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 |
| `venues/kucoin/src/identity.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 |
| `venues/kucoin/src/lib.rs` | `b7236676fb5159713524fe49af418c62b87162e905dab02bef27a1b2badde5a3` | 2 |
| `venues/kucoin/src/lib.rs` | `bac8c36ebafa2042f49567b5bfb145dc6285c54d802542bcef1aa1d54d94955a` | 1 |
| `venues/kucoin/src/lib.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 |
| `venues/kucoin/src/lib.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 |
| `venues/kucoin/src/lib.rs` | `f6fd9085f9b1b305170e32707e6244a6773605cd81588f17ab52b6a17cb6e08a` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `0597ddd241d06b439fbe4e473127392d5b647a9d4437fc1a4b903b974f21bf94` | 2 |
| `venues/kucoin/src/secondary_tests.rs` | `159f5704bcac34411f65ad8cc4e2e106a4d8575fedd68580800396d53c3c9ce7` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `65ff396da1ed90ecf05d1827e7770baf9314abb1ab900b929a600b9142d9b401` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `7af1094316bcf84e9fbbbf0a9424df23be5feac988016338a63a93369f08ae3b` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `856927040f21fefa7ac348eca86cf760ea2b34f71b2a42785d71a7eaf490b319` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `8eef6157c4aa384dd6cf16bd4fb4d8f28963578b97454ea94d8d63d988497812` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `a65fdc4b75f9441e9693fd7c31ac9019dbf1d2ca3215e6e557105b2e9223db3f` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `a7295c3cde6850a230bb760a9465683ccd718556f87cab40a9283bb16899df53` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `e57766872e3db7f45d338b12e52551e4cafa21312459083c934b95009cae10b9` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `2485f9b0677fc07b9dc4f3202a0dbfc7dd3b13e82476cd509be3282e8ce4d863` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `417cd86311e6261c7b1911896dc467829c4cfafd1fc48956cfbce153fbc17582` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `44284ddcfc94ad5da1cd7f5576dc01737ad7eeb0f75aa49c95bf42849d08790a` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `520659ca128f10f1f3f1a724ed704b116b8f56da364dcd28fd6aaabfc25c5872` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `b7236676fb5159713524fe49af418c62b87162e905dab02bef27a1b2badde5a3` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `bf606094d92ae7fcf6e97dd3f9f690ff00d2954558e1c92671c1c455c0535065` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 |
| `venues/kucoin/src/spot/mod.rs` | `de4e8a9f01fa2526af01b897df7e71eb6422d5bda2faabb22527e73503e6a5a6` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `f9cbb2adfc98c4ca1e0d379b2b1c90b52a3e91fd61f8beaebb998423812ddf8d` | 1 |
| `venues/kucoin/src/spot/trading.rs` | `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 |
| `venues/kucoin/src/spot/trading.rs` | `33e21f13f871bdbb7268e01da0006a69a903f16f2d8daa34972b49211d81ee6f` | 1 |
| `venues/kucoin/src/spot/trading.rs` | `d0677fb68bd1966d34f4d686f19ab321881f696d903f33b76f0ad52aac4b3902` | 1 |
| `venues/kucoin/src/spot/trading.rs` | `fc46e4dc9fea9911455ee3e3f7822ea0a3c903d8314f5fa82214941c0a64d9f0` | 1 |
| `venues/kucoin/src/trading.rs` | `a3929c62720c7893d6477453bb8092ee1dc649a8b5024ffd9867b8715c3afc2f` | 1 |
| `venues/kucoin/src/trading.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 |
| `venues/kucoin/src/trading.rs` | `e2059285572e975717f09275846f33678d7bf1e78ddd4d40100025ab825c118f` | 1 |
