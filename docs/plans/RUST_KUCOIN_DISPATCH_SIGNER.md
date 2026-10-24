# KuCoin futures and Spot opaque dispatch signer cutover

Owner: codex-9czdjh. HMAC/native reviewer: claude-4yjlw0. Credential/role reviewer:
claude-m26ps4. Base: actual `77f12408947`; branch `feat/rust-rc-next-slice`.
This refresh replaces the historical ba3d/56-site plan. Plan and original REDs
only until both reviewer scopes are CLEAR; no source or inventory transition is
approved by this document.

## Scope and current accounting

The exact checker inventory is **46 occurrences / 42 path-local fingerprints**.
All 46 RC sites are listed below; the separate signing inventory follows too.
The signer/broker cutover removes their custody gate;
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
land or the authorizing maintainer claude-ddi2il grants exact composition. Do not copy their 27-path
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

The native stamp is integer milliseconds: `wire_stamp_ms(now) as u64`, rendered
as decimal text (sign.rs:51). Pin this exact conversion and reject unsafe/non-finite
clock inputs through shared validation; do not silently change other families.
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
Futures trading.rs:867-876 also reissues a write immediately ONCE on 400005,
using the retained retried flag. It must prepare a fresh atomic provider batch
with a new receipt and recomputed request/passphrase/broker MACs, without a
new timer or renewed command root. Native 400002 is AuthInvalid, not a clock
retry. Spot gains no generic retry.
A two-attempt control must compare changing request/broker MACs and unchanged
passphrase MAC, each genuinely recomputed in its own batch. Execute this
control through the genuine Futures write retry path as well as read paths.

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
4. Rescan exact candidate. Obtain claude-ddi2il's explicit grant for any new
   STAGING_BOOTSTRAPS or SIGNER_POLICY_BOOTSTRAPS maps before editing them. No
   numeric shrink is promised from an unbuilt funnel. Keep frozen call expressions stable for
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

## Current-tip execution and review receipt

Plan checkpoint36943c51312 is role/setup CLEAR from claude-m26ps4. Independent
plan/test/preservation review is CLEAR, architecture gate No. The exact seven
historical native tests remain a token-identical prefix; independent review
recomputed the seven fixed HMAC vectors. No production, construction policy,
shared custody code or inventory file has changed. New broker-ref/alias types
must have redacted Debug: opaque names may appear, key/secret/passphrase/broker
material must not. Test this at native Options and installed alias boundaries,
matching existing Sensitive/credential redaction; no raw provider material Debug.

Native tests at checkpoint5759e49f654 and the broker review delta below execute
on unchanged77f production:

- With Spot: 15 compiling tests, **5 assertion REDs / 10 working GREENs**, zero
  ignored. Four REDs observe zero provider batches where one is required:
  Futures catalogue, broker catalogue, Futures identity and Spot identity.
  The Futures write 400005 reissue RED observes zero batches instead of two.
- Without Spot: 13 compiling tests, **4 assertion REDs / 9 working GREENs**,
  zero ignored. The two Spot-specific controls are not compiled in that mode.
- Isolated broker role: 10 compiling tests, **8 NEW-CONTRACT runtime schema
  REDs / 2 existing working GREENs**, zero ignored. The first missing
  `kucoinBroker` role/policy decode rejects; downstream trim/alias/MAC assertions
  have NOT executed and are not claimed as independent semantic defect proofs.
- Working controls include actual native ordered MAC/header bytes, complete and
  partial broker triplets/default/false verify, selected read-key broker MAC,
  configured Spot public unsigned behavior, keyless Futures unsigned behavior,
  a genuine atomic two-MAC provider batch, and existing ordinary/role isolation.
  The write retry control pins stamps1234/1235, changed request/broker MACs and
  equal passphrase MACs; raw-query vectors and native Debug redaction also pass.

Strict test-target Clippy passes for both packages after correcting two
get-then-check assertion-style warnings; test meanings are unchanged. The role
algorithm matrix uses an exhaustive successor match over every Algorithm variant;
a new variant requires a match arm before the target can compile. It covers all
eight current non-HmacSha256 alternatives.
Full local banned-patterns and request/signing/socket ratchets, whitespace checks
and prose/test secret scans pass. No full native suite, WASM build, replay, size,
production provider integration or actual broker landing is inferred here.

The draft deliberately retains executed REDs. HMAC re-review at5759e49f654 is
CLEAR from claude-4yjlw0, including independent reproduction of all three test
counts and the write/raw-query controls. Actual prerequisites or an explicit
shared composition, production source review and fresh CI remain
separate gates. No source is authorized solely by these test receipts.

## Separate signing inventory and migrated-family flip

The request table above is exhaustive only for RC provenance. The independent
`signing-legacy-sites.json` snapshot on77f contains **41 occurrences / 38
path-local fingerprints** for KuCoin. This is a different ratchet and count.

| Path under sdk-rs/ | Occurrences | Fingerprints | Required disposition |
| --- | ---: | ---: | --- |
| venues/kucoin/src/history.rs | 1 | 1 | Remove raw credential decision through selected opaque custody |
| venues/kucoin/src/lib.rs | 16 | 15 | Erase generic/raw primary/read/passphrase/broker material and local signing |
| venues/kucoin/src/secondary_tests.rs | 8 | 8 | Preserve original assertions through trusted fixture setup; retain legitimate test oracles explicitly |
| venues/kucoin/src/spot/mod.rs | 16 | 14 | Erase raw private helper/state; keep public requests unsigned |

Production must add `kucoin` to the migrated family set in check-signing.py and
its negative-control loop, exactly as Bitrue2029 does. That flip also starts
scanning sign.rs and sign/ descendants, which the current legacy-family rule
excludes. This exposes **16 currently matching sign.rs lines / 13 distinct
line fingerprints** (enumerated below). Those are an additional scanner-visibility
obligation, not 16 new production sites introduced by the cutover. Remove native
primitive/secret holders, move any retained oracle behind test configuration, and
freeze only legitimate reviewed test/metadata matches. Run negative controls for
new and duplicate sites; preserve every other family map/order. Request actual
bootstrap grants where required; no broad exception or guessed final count.

The following separate signing manifest map is exhaustive at the frozen base:

| Path under sdk-rs/ | Signing fingerprint | Count |
| --- | --- | ---: |
| `venues/kucoin/src/history.rs` | `2896204db3e09d1a447d95223c9c07b959206c1049fb573f85e5cf184c3e7a8a` | 1 |
| `venues/kucoin/src/lib.rs` | `0219bc24bc618541b8bf84b12a617687f15c479a2a90d9a765b85c28878bfd7c` | 1 |
| `venues/kucoin/src/lib.rs` | `1f18bec61f449c3913ae221613ed165c7a7569b05bd6c1f99bdd0360f5259e57` | 1 |
| `venues/kucoin/src/lib.rs` | `20861b679b7c10377687b03415921648e95232082ba70cf253fa5e92dd5cce5a` | 2 |
| `venues/kucoin/src/lib.rs` | `239d8bb1b21eae866af31bbf63a30b5b06ab0bea27aa256fe4a260fd801ac53f` | 1 |
| `venues/kucoin/src/lib.rs` | `31c4a378f691bfb9a36c5bfab06eefdbc87243936860af8d8bc1ff86e6060e84` | 1 |
| `venues/kucoin/src/lib.rs` | `370d47b1b3dca65ba5ffee892a08785f5833f4db7d5264872d4e88e5e04c567c` | 1 |
| `venues/kucoin/src/lib.rs` | `3d0b1da9613f86f2f90197fe3d09161f9b775bbcd3eb7d61c0bbf4caf0f10573` | 1 |
| `venues/kucoin/src/lib.rs` | `56b2f68f6e9d2a7e8d8d9c12697428fa0218289b90f6689ffb620baf490041e4` | 1 |
| `venues/kucoin/src/lib.rs` | `84a58c8cf37ba0b1dc8a11fa97c888c268fabc018cc0cc0213e8ad689a5a14df` | 1 |
| `venues/kucoin/src/lib.rs` | `998051a7b82dfb9002576f90c080a63ad097bd78ea4f1b21f8ef67b3e1d30152` | 1 |
| `venues/kucoin/src/lib.rs` | `b385287b7903b95152ecb2025d782dbe45d7ac64448fef388e9ac6056188ed13` | 1 |
| `venues/kucoin/src/lib.rs` | `b44b8c98339cc77eb4fd6a0ec89fa0c5a55f0dd263d57dfa5dc6fc3031594c04` | 1 |
| `venues/kucoin/src/lib.rs` | `c04fc85172b971840f73318a9736832e498f2b19c4e7d132a0d6407263f60584` | 1 |
| `venues/kucoin/src/lib.rs` | `f2743a2be9b140c75420eb6b472c32912171665878b5586c6d9ca53853c62a9f` | 1 |
| `venues/kucoin/src/lib.rs` | `f5337bace3eb12b97fe8ec3b8ff28168506324b89eb83b624688404891673788` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `2791b1cb0b8e1d24249ff6ba09597949814014f3dd4e007efa0cd2344e716052` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `312e63f384b36461835c77c42018f6ed4813059e71a2aa5ad975a4a76ae547ba` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `5197051b828db9d78a27c00482911a29712a6d9dad1ea40b1c4e897286143dbc` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `5b80fd58f5f9859eef7ed9d57eaf6a9087d5057f87b1e89c497e756cf894df11` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `5bcd752556bac2b8348e1071d147565c915e158f4c0d016782795d1bd121218a` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `8fde5a2b784059f7db8e376d194270496958f22cf05923decb29eddc607cf153` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `96590eff7cf76980f83548cc0da7a59621eae3391da67fe9863e68bafd8de327` | 1 |
| `venues/kucoin/src/secondary_tests.rs` | `d52b3ae1b0690eae15c7aff4c4bf222fa70a0a4304616eaf812cd96cf0dec9e2` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `05816b24b2ee1edaf364146f9016d0d6f10a08b56b0ef613234cdb9bd4f13f4b` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `0c34e33203af9f5eb6bf128257c8bdf5caaab04d5808e870fcb70cc22a59a7c6` | 2 |
| `venues/kucoin/src/spot/mod.rs` | `178d78c26cfc9c6e19f6fea3deec67a29da33635fab918a8742961cd75ee6e67` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `20861b679b7c10377687b03415921648e95232082ba70cf253fa5e92dd5cce5a` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `293e48984f4ee24823325b8a5ff7cad1ba41975db53422d477d9ed7793db178a` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `370d47b1b3dca65ba5ffee892a08785f5833f4db7d5264872d4e88e5e04c567c` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `74d7492f0b6267a2f75ee1fb7874a090b6cd10d68fd1ed35224c61491cf900f3` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `84a58c8cf37ba0b1dc8a11fa97c888c268fabc018cc0cc0213e8ad689a5a14df` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `9b258d4209b1982036d0bf8c004a7a1eb3eccabf81a295ecdc197d522623498d` | 2 |
| `venues/kucoin/src/spot/mod.rs` | `b385287b7903b95152ecb2025d782dbe45d7ac64448fef388e9ac6056188ed13` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `bb0b6519a9602671c05eaeb09cebec736bd1256fc71ffdd6a2dce4e2786c1e33` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `bcc0c3e0c84201bd384ce04502323c3b473579c97451aa96065d9e9371f50da0` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `f2743a2be9b140c75420eb6b472c32912171665878b5586c6d9ca53853c62a9f` | 1 |
| `venues/kucoin/src/spot/mod.rs` | `f5337bace3eb12b97fe8ec3b8ff28168506324b89eb83b624688404891673788` | 1 |

Newly visible sign.rs lines at the migrated-family flip (original77f bytes):

| Line | Signing fingerprint | Disposition before source freeze |
| ---: | --- | --- |
| 2 | `48fd4540f198706c9bb61b9937bb5d67341bd69e331c1bc10b6d7a6385008710` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 17 | `6fa4402de35a1a6f3f80e355ec7e327d4bbdee53c43dbb7c41858fe087f29c61` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 59 | `659735b719f8acf3fdea49dfc2f0e5ffc8828897283ed8cee326ef0c38727b7d` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 60 | `8c1c69ce9f10f73666ef4c21123abe7c29fe9ed22644347ff6b30cfc38b42b53` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 67 | `659735b719f8acf3fdea49dfc2f0e5ffc8828897283ed8cee326ef0c38727b7d` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 68 | `8c1c69ce9f10f73666ef4c21123abe7c29fe9ed22644347ff6b30cfc38b42b53` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 86 | `83fa4a266d9ff7985b9c77292f2a8c45b6ebbff517eef91aade3a4875f3954df` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 106 | `736bafffd2886401fcf5c57cdb6d41e45493f3954ba159693e1258fd60718c05` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 145 | `736bafffd2886401fcf5c57cdb6d41e45493f3954ba159693e1258fd60718c05` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 181 | `84713aca2ca2e28c8baa26787ac22d860e36cdfbe247a06b32574c5c7c7788bc` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 182 | `3d0b1da9613f86f2f90197fe3d09161f9b775bbcd3eb7d61c0bbf4caf0f10573` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 183 | `0219bc24bc618541b8bf84b12a617687f15c479a2a90d9a765b85c28878bfd7c` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 184 | `56b2f68f6e9d2a7e8d8d9c12697428fa0218289b90f6689ffb620baf490041e4` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 185 | `239d8bb1b21eae866af31bbf63a30b5b06ab0bea27aa256fe4a260fd801ac53f` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 187 | `1f18bec61f449c3913ae221613ed165c7a7569b05bd6c1f99bdd0360f5259e57` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |
| 193 | `d4687b2e7809352d30f7f173c117a678d8293f1c9a4377d22cdabe864e638f58` | Remove production custody/MAC work; retain only genuine test or diagnostic evidence |

## HMAC review corrections and strengthened RED checkpoint

The 36943 plan was NOT HMAC CLEAR: 4y found a P1 missing signing inventory and
P2 omitted Futures write 400005 retry, plus integer-stamp/maintainer/redaction
wording. The separate 41/38 map and newly exposed sign.rs16/13 scan above address
the inventory finding without altering either manifest. The plan now names
claude-ddi2il as composition authority, pins native integer wire milliseconds,
requires redacted Debug and explicitly includes the immediate one-write retry.

At test checkpoint5759e49f654 on unchanged77f production:

- With Spot: **5 assertion REDs / 10 working GREENs**, 15 compiling tests,
  zero ignored. The additional RED sees zero provider batches instead of two on
  the actual Futures DELETE-write 400005 reissue.
- Without Spot: **4 assertion REDs / 9 working GREENs**, 13 compiling tests,
  zero ignored.
- Broker role: **7 NEW-CONTRACT runtime schema REDs / 2 existing working GREENs**,
  9 compiling tests, zero ignored. Added alias Debug and the complete eight-
  alternative algorithm matrix; later assertions remain unexecuted until schema
  support exists.
- The native write control sees two immediate attempts at stamps1234/1235 with
  identical method/URL/body, changed request/broker MACs and the same passphrase
  MAC. Additional Python-independent raw-query/passphrase/broker vectors from
  4y's review pass, including literal plus and space. Native Options/BrokerOptions
  redacted Debug exposes none of the synthetic primary/read/broker material.

HMAC re-review of checkpoint5759e49f654 is CLEAR from claude-4yjlw0. Its two
non-blocking documentation findings are resolved above: current counts replace
the earlier receipt and claude-ddi2il is named as bootstrap grant authority. No
source, credential policy, checker predicate or inventory change is part of this
draft.

## Role/setup review follow-up

Role/setup at dffc84bb849 is CLEAR from claude-m26ps4, including independently
recomputed broker vectors and the schema-RED distinction. Both non-blocking
control requests are addressed without production changes:

- Secondary-only setup with AliasPrimary succeeds, leaves primary absent and
  creates no broker ref. This expectation matches the reviewed setup insertion
  and optional alias validation in62377e9368e. The new test currently fails at
  policy decode; its downstream assertions have not executed on the landing.
- The algorithm matrix derives its sequence from an exhaustive Algorithm match,
  so adding an enum variant requires an explicit test update at compile time.

The refreshed isolated target compiles: **8 schema REDs / 2 working GREENs**,
10 tests, zero ignored. Native test files remain byte-identical to the HMAC
reviewed checkpoint; their 5/10 and 4/9 receipts are unchanged. Scoped strict
Clippy and whitespace checks pass. Independent test-delta review is CLEAR,
architecture gate No. Actual shared prerequisites or explicit composition still
gate production; this is no claim that the broker contract executes successfully.

## Approved source composition

claude-ddi2il explicitly approved local source composition on combo-5b
b485b4e5fd9 and #1990's owner-composed0f276adbe22. The latter retains the closed
role resolver and ordinary asymmetric declarations together; its review/CI and
actual landing remain separate gates. No resolver was locally substituted.

The reviewed broker deltas from d545ee95bb4/62e412720d3/62377e9368e now compose on
that pin. Mechanical conflicts retained incoming base58 support and controls,
added broker assertions and trim-aware generated expectations; shared Sign and
can_sign resolver bodies remain unchanged. The isolated broker target now runs
all ten controls GREEN, zero ignored, including previously schema-gated alias,
trim/BOM, missing-primary, algorithm and redaction assertions. This is local
composition evidence, not an actual shared landing claim.

Recipe preparation is not yet wired into the venue. Independent source review
found a valid negative-zero formatting edge and an untyped result-index compile
error. The recipe now uses native validated u64 decimal conversion and an
explicit usize index. The appended native zero/negative-zero broker/header MAC
vector control passes; integrated recipe execution still follows wiring.


## Local production composition checkpoint

The approved #1990/combo-5b composition now carries opaque KuCoin venue Options,
the reviewed atomic recipe and real provider setup for both aliases. Broker
material is consumed once by trusted setup; its secondary identity aliases the
primary's role ref. Native Options retain partner/name/verify metadata only.
Unsigned Futures calls retain their native route field; Spot public stays unsigned.
Identity uses shared SelectedRead; other aggregate/session owners retain their
explicit staging scope pending their own lifecycle conversions.

The scoped KuCoin + Spot native suite executes **392 PASS, zero failed/ignored**.
These are test executions, including repeated native-oracle controls mounted in
multiple integration targets, rather than 392 distinct custody proofs. The
adoption target has 23 controls, including seven provider-result tampering cases
inside one control and deferred atomic-batch success/refusal/late-result cases
inside another. Deferred signing traverses genuine route Prepare, provider Sign,
Bind and release Claim; no Context, provider success or receipt is fabricated.
Feature-off unit/adoption targets execute another 40 PASS, zero failed/ignored;
scoped strict Clippy passes for KuCoin with/without Spot and for the registry
target, and both packages pass formatting.
The eight registry construction controls pass after shared constructor validation
was deduplicated. The added public-only/passphrase-less-primary control drives
real registry construction and public bullets, depth and candles, pinning unsigned
headers and no signing failure even though the unused primary ref is registered.
SDK broker retention had two original assertion REDs before the
capsule mapping; those plus existing local-signer setup controls now pass 64.

Bounded independent source review is CLEAR for failure disposition and trusted
broker wiring: provider refusal is strict for history/funding, never transport
retry evidence or accepted-prefix recovery. Native account refusal releases its
existing owner without spending transport retries. Spot write diagnostics precede
terminal; Futures retains typed unsent errors through native aggregate handling.
Native fixture adapters use real provider preparation and retain wire/result
assertions; invalid identity controls alter opaque references before construction,
without manufacturing notices or silently normalizing invalid secondary input.

Own request inventory is **46 occurrences / 42 fingerprints -> 37 / 35**. The
remaining physical staging funnel keeps its native fingerprint; no request site
was added or renamed to evade the scanner. Own signing inventory, including the
migrated-family scan of sign.rs and sign/dispatch.rs, is **41 / 38 -> 15 / 13**.
The recipe's three HmacSha256 declarations and test-only debug sentinel are
explicitly inventoried; the test oracle is outside production source. All
foreign map entries and their ordering remain unchanged. Credential-selection
bookkeeping shrinks from 15 to 4 KuCoin sites through real opaque/SelectedRead
adoption; no alias hides a production selection. One test function name was
changed to avoid accidentally matching the banned `preserve_order` token, with
all its native assertions unchanged. No STAGING_BOOTSTRAPS
or SIGNER_POLICY_BOOTSTRAPS allowance has been changed or claimed.

This is local composition evidence, not a freeze or actual shared landing claim.
The shared alias policy tightening is plain-merged from the credential owner's
independently reviewed 0d162668911 (parent exactly 265aee2c35f). Its original
assertion RED observed a missing secondary alias and is now GREEN. A further
registry control removes a host-supplied secondary alias, then proves both aliases
acquire the primary's exact ref and emit the independent native read-key broker
MAC with all four broker headers. The separately reviewed b39acfb805d is also
plain-merged: malformed supplied Optional extra refs are refused before alias
mutation. Registry controls pin empty/257-byte front-door refusal. Public-only
complete opaque identities explicitly require a provider even without a
passphrase; absent provider is typed InvalidInput, while installed provider
produces the native unsigned public wire. Actual prerequisite integration, final reviews
and full CI remain required. The current landing 9d344d541d5 was plain-merged
at 265aee2c35f; all reviewed
KuCoin and shared signing/custody blobs remained byte-exact. Four docs conflicts
kept incoming text followed by each unchanged approved composition suffix. Own
exact inventories and all 15 request checker controls pass. Foreign credential
selection drift already present in combo-5b remains with its owners; no foreign
map update is authorized by that drift.

## Wrapper CI correction on the reviewed 63dc line

Ordinary CI 40100969710 failed on three wrapper fixtures, independently
reproduced locally as 3 failures and 64 working tests. The Aster STP matrix's
KuCoin case and the KuCoinSpot SETTINGS-frame fixture omitted the now-required
native passphrase. They now supply a placeholder passphrase without changing
any command, STP, account or Store assertion. Aster cases remain unchanged.

The KuCoinSpot constructor fixture expected raw material in engine-options JSON.
It now records the private setup argument and pins key/secret/passphrase absence
from both engine options and retained exchange.options, with the same material
present only in the trusted signer capsule. No production or shared setup rule
changes. All 67 tests across these three files pass from the safe-cex package,
with one Vitest worker. ESLint deliberately ignores these test files; no lint
execution is claimed for them. An initial unsupported Vitest --minWorkers flag
stopped before tests and is excluded from the RED receipt. Fresh CI is required;
actual #1990 landing order remains held, and 78758 remains unpushed.

The wrapper fix is followed by plain merges of actual 0258ed21afe and16f776ffb82
(the incoming landing advanced during validation). No unlanded 5db dependency
was merged. Reviewed KuCoin runtime/tests and engine/crypto signer files remain
byte-exact63dc. The three wrapper files remain byte-exact their tested fix.
Manifest deltas against16f are KuCoin-only; docs are incoming plus the existing
KuCoin/asymmetric composition suffixes once. Full bans, request/signing and
credential-selection ratchets pass. Fresh PR CI will certify this fix head;
#1990 actual-landing composition remains required before READY/landing.
