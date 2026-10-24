# Bitrue dispatch signing migration

Owner: codex-9czdjh, reassigned from claude-4yjlw0's #1718 queue. This
plan/test-only branch is feat/rust-bitrue-signer-migration, cut exactly from
actual 85e155faebb5b1b4925b1806875ac6581167b9c0. Production stays held.
The inherited HMAC plan33967696978 is historical; source below was re-derived.

## Inventory, collision map and cutover gates

The checker inventory is33occurrences/20fingerprints/four source files. Own
#1993 removes three occurrences/two fingerprints from trading.rs (33/20->30/18).
It owns trading.rs and tests/local_command_refusals.rs until landing; this branch
will not edit them. The first checkpoint changes only this plan/index and a NEW
isolated tests/dispatch_signer_adoption.rs. No source, manifest, capability,
construction, remote-signing admission or checker exception changes now.

Current shared claims include registry/src/lib.rs in #1891/#1915/#1974/#1990;
check-request-context.py and check-signing.py in #1915/#1974. Recheck all open PR
file lists before later source phases. Production waits for actual #1993,
reviewed prerequisite APIs and an authorized shared-file composition window.
No active unpublished Bitrue source holds remain with the previous HMAC owner.
The successful one-way no-op remains with shared #1989; signing does not bypass it.

| Path relative to sdk-rs | Occurrences | Fingerprints |
| --- | ---: | ---: |
| venues/bitrue/src/account.rs | 8 | 8 |
| venues/bitrue/src/lib.rs | 11 | 2 |
| venues/bitrue/src/secondary_tests.rs | 2 | 2 |
| venues/bitrue/src/trading.rs | 12 | 8 |

The exact baseline below includes conservative pending.request fields and macro
matches; these are not eleven physical sends. Keep native assertions and
continuations. No rename/move may be claimed as a genuine request-site prune.

| Source path | SHA-256 fingerprint | Count |
| --- | --- | ---: |
| venues/bitrue/src/account.rs | 187ae7a49b233001f96c827642f4839ad9e3ea4fd81b9b62848d3900055afced | 1 |
| venues/bitrue/src/account.rs | 1b3029882eb1af7175e494797938adedfb4b3c6cbd38573c98c2e4940415d89b | 1 |
| venues/bitrue/src/account.rs | 4f19e9de3124938051c48304f8476b059f9928dd6f790df05a27c9f821512d93 | 1 |
| venues/bitrue/src/account.rs | 985d5251ce2c2f3e0d27d6c46def63a68db127135e27d9e2917a1923e354a48f | 1 |
| venues/bitrue/src/account.rs | adebf191cda07505ad7d8529e3d1e50353f7a3251988cfcd01ba53d93563b444 | 1 |
| venues/bitrue/src/account.rs | c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548 | 1 |
| venues/bitrue/src/account.rs | c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa | 1 |
| venues/bitrue/src/account.rs | df3c1d81952c8a455f966081ae19fcca364d48093c5893e69a82108cac16906d | 1 |
| venues/bitrue/src/lib.rs | 34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc | 1 |
| venues/bitrue/src/lib.rs | 8d7195dd58da632e34e4dc75d236e49b6d1b89cc10c243ec7e670ec59b511d26 | 10 |
| venues/bitrue/src/secondary_tests.rs | 14ff7b932a6042d0adee80518f703a27a5e51c41add140215703e93820dc3475 | 1 |
| venues/bitrue/src/secondary_tests.rs | 496bfb28799c729f0f335ccce50062d6a9ba06a581527e6a36a638ac44c84dc8 | 1 |
| venues/bitrue/src/trading.rs | 09d284b47513158ba424efe6b85ef618c3f2649072f4211be5a3ec043ecc0e88 | 1 |
| venues/bitrue/src/trading.rs | 2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5 | 2 |
| venues/bitrue/src/trading.rs | 5e1ebc068cca89013b259957b834e34abaec73947a047f535da9894eb0b5c254 | 1 |
| venues/bitrue/src/trading.rs | 6d2c051bb24db8acd1986f4e47c2b6dbede5aa9dfff7fb877e64ef194107bd53 | 4 |
| venues/bitrue/src/trading.rs | afb3c3744f13da9606f5f456bd95797992a932cf227eb74b2fe4405e3c64ee98 | 1 |
| venues/bitrue/src/trading.rs | b780b823ba70712169e1be98fecae4d2a2655f7ca35f27bea4b99f09bd684696 | 1 |
| venues/bitrue/src/trading.rs | cb0a44751a1560cceb6c0a90e523713002ad5efbdf5418c58f0132ef2c94fce0 | 1 |
| venues/bitrue/src/trading.rs | dcb84cef5dcb71801d2b1b9562fcfc84b0431681917a0b6b9d2314f2e050630e | 1 |

## Native canonical bytes and atomic recipe

Actual source: sign.rs:12-18 signature; lib.rs:269-340 send/issue;
lib.rs:839-854 response resend and920-949 streamed-failure resend/transport retry;
account.rs:531-540 listen POST,811-820 keepalive PUT,1762-1802 token acceptance.
Token retirement is local generation/key cleanup at account.rs:542-553.

One SignRequest and one receipt per physical preparation:
1. Present the selected Identity.credential API-key ref.
2. Sign HmacSha256 using its signing ref over timestamp text + UPPERCASE METHOD
   + exact canonical path/query + exact body (empty when body is absent).
3. Encode the32raw MAC bytes as lowercase Hex at header rendering only.

No passphrase presentation or MAC is used; a configured read passphrase has no
signing role here. API key/secret bytes are untrimmed. The initial native timestamp
is cx.now.max(0) as u64: truncate nonnegative milliseconds. Recipe preparation
uses the shared protocol clock, preserving the nonnegative/truncation boundary
under a reviewed clock policy; no new local clock, timestamp counter or expiry.
Keep raw scheduling time and protocol signing time separate.

Relative URLs use https://fapi.bitrue.com. Native absolute listen URLs use
https://fapiws-auth.bitrue.com; sign /user_stream/api/v1/listenKey (plus the
validated token on PUT), not scheme/host. Native issue strips the first .com;
the recipe binds the two real native origins and their exact canonical suffixes,
without broadening URLs or percent-decoding/re-encoding query bytes. Preserve
raw values, duplicate/empty query components, plus and Unicode wherever native
builders permit them; do not fabricate an endpoint to exercise a genuine caller.

Ordered unsigned headers: Content-Type:application/json, X-Referer:Tealstreet.
Ordered signed suffix: X-CH-APIKEY, X-CH-TS, recvWindow, X-CH-SIGN.
recvWindow is HEADER-ONLY: default20000, registry normalization1..60000, and no
new recipe-side clamp or addition to the prehash. Before prepare AND apply,
attest the exact method, allowed origin/full URL, body, native static header order
and empty signer_headers. Validate exact receipt/count/key bytes/MAC size and
Deny callback-header policy before applying any result. Delayed, malformed or
refused provider output must release no partially signed request.

## Trusted selection and native ordering

Ordinary private GETs use secondary only when primary key+secret are present;
without a read key they use primary. Actual writes and listen POST+PUT use primary.
There is NO native DELETE: cancellations are POST /fapi/v2/cancel and listen-key
retirement is local. Trade-owned lookup GETs after writes and myTrades still use
secondary; do not promote them solely because a mutation initiated them.
Catalogues are explicitly PrimaryRead(Catalog), even though GET; this is an
operation declaration, not a new method-based selector. Use shared Slots and
SelectedRead for eligible plain command/background reads. Root/page consumers
use actual #1983 (a2533f5761f) and actual #1976 receipt expiry where appropriate;
unsigned/public reads never enter missing-selection private refusals.

Native markets always force Primary egress, including keyless and public-only
startup. Keyed public-only catalogue STILL has primary signing headers; private
startup stays suppressed. Native other request routes are None; declared
Selection egress is an intentional policy transition that needs exact approved
SIGNER_POLICY_BOOTSTRAPS maps. Missing primary material with a configured read
key still leaves catalogue unsigned and suppresses private startup. Preserve
read-key validation before ALL startup I/O, even public-only. Empty/partial
primary Options edge cases must be pinned against trusted construction before
cutover rather than silently reclassified. Registry currently forwards raw
Options; construction has no Bitrue-specific dispatch admission declaration.

The valid FetchMaxLeverage path checks symbol before private credentials and
then issues one bracket GET. FetchLeverage joins a positions/account read;
retain its per-owner group lifetime, projection and deadline requirements using
fe7lgm's background execution API, not a local fan-out authority. Fills is one
native recent-page read with row/coverage validation, no invented pager.

## Resend, lifetime, session and error gates

Native -1022 currently triggers one signature resend on parsed response code,
INCLUDING unsafe408/5xx; streamed list errors also recover by parsing a formatted
engine failure string. These are recorded P0/P2 debt, not approved semantics.
Managed production must use one Bitrue CodeTable and the shared classifier facts:
only an eligible RefusedResend permits resend;408/5xx/transport remain unknown,
not a local status ladder. Keep accepted IDs as structured journal Partial;
malformed2xx is unproven/Unknown, not None (classification rows8/21/22).
Existing native mappings to move into that table: trading.rs:1183-1189 proves
-1022/-1102/-1121/-2017 refusal only at200..499 excluding408;1515-1522 maps
401/403/429,408/5xxUnknown and -1022AuthInvalid after the single signature retry.
Keyless catalogue -1022 currently gets one unsigned reissue as well; preserve its
eligible native read behavior without fabricating an Identity or signing authority.
Move existing parsing solely to native-code/message extraction, never classification
from engine-formatted strings. Negative controls pin -1022+HTTP500 emits no resend.

For EVERY request re-issue, both eligible signature resend AND eligible transport
retry, native uses max(nonnegative now, previous timestamp+1), including a fixed/
backward clock (lib.rs:946). Each attempt needs a NEW atomic preparation,
fresh receipt and MAC, never cached headers. A previous value must come from the
exact genuine shared SigningRelease receipt for the released attempt, with original
selection/account/generation/egress retained. Existing DispatchContext.stamp_ms
alone cannot express the fixed-clock+1 rule. The exact trusted receipt-bound
minimum-stamp API needs shared clock/dispatcher review before production; do not
implement a venue-owned timestamp allocator or infer retry permission from metadata.

Native retry flags are independent: one signature resend and up to3transport
retries for eligible NON-market GETs or listen POST at its EXACT full LISTEN_URL
only; the signature floor applies to these transport reissues too. Catalogues explicitly
skip that immediate branch and retain startup backoff. Malformed-list/UTF-8 failures
and a second parsed -1022 do not enter transport retry. No generic uncertain
write/PUT replay is added. The two budgets do not reset one another. Native
per-attempt HTTP default is20s; an eligible non-market chain can issue up to5times. A single20s SelectedRead root would
narrow that chain. Freeze the native physical/root/queue declaration with core
review before conversion, with near20s response/retry controls; do not assume a
new100s root or renew a root by renaming work. Keep retries explicit staging
until the reviewed shared policy fits. History keeps its original caller root.

Listen acquisition and keepalive are SESSION mutations, never disguised Read or
public requests. Session/generation owns token_pending and restart; preserve valid
alphanumeric/-/_ token bytes, generation mismatch discard,30minute keepalive,
20s supervisor idle and native reconnect/poll cadence. Preserve public suppression,
private startup independence,2s account polling, order backoff/queues and settings
revision fences. No separate WS HMAC login exists; WS URL carries the issued token.
The public socket codec/subscription/FIFO read implementation stays untouched.
Any session/Plan/background lifetime gap returns to its shared owner; signing is
not readiness for zero legacy, Plan history or G2 provenance.

## Phases, original controls and exact transition gate

1. Commit this native plan/inventory; independent HMAC, source/adversarial, caller
   and full-map reviews. Production remains held on #1993 and shared gaps.
2. Commit compiling original genuine-engine assertion REDs before wiring: keyed
   primary catalogue; secondary bracket read; primary setting POST; absolute listen
   POST and PUT; fixed/backward-clock -1022 fresh provider preparation; refusal of
   -1022/500 replay. Working controls pin exact native HMAC vectors/header order,
   selected keys/routes, static unsigned keyless catalogue, no recvWindow in MAC,
   read validation/public-only behavior, one eligible resend, independent mixed
   signature/transport budgets, catalogue backoff, malformed-list/UTF-8 retry exclusion,
   token keepalive/obsolete-session completion and both method/body bytes. Installed trusted RecordingSigner must have an independent
   positive work control so provider0 cannot be blamed on a broken fixture.
   These tests use existing public APIs; missing-API compilation is not RED evidence.
3. Test-only recipe/oracles may proceed after plan review. Production custody and
   caller adoption wait for actual #1993, shared clock/selection/lifetime fit and
   claimed shared files. No raw REST fallback, signer-only carrier or disabled path.
4. Freeze ACTUAL full OLD->NEW fingerprint maps before requesting explicit owner
   approval for STAGING_BOOTSTRAPS and its approved-transition test. Current issue
   already has one stable cx.request(method,url,headers,body,pending) expression
   (34b6cbe60cec x1). Adding cx.sign_http(id,Box::new(recipe)) would add
   6cf35cca0977 x1: CANDIDATE ONLY33/20->34/21, after #1993 30/18->31/19.
   Managed read adoption may change that candidate. No growth or exception is
   authorized by this plan. Keep native request expression stable for real later
   pruning; recipe-factory rehashes require separately approved SIGNER_POLICY maps.
5. Coherent opaque custody cutover removes every production raw material/MAC site,
   includes Bitrue sign modules in signing ratchet and preserves test oracles without
   hiding their matches. Hosted compatibility family/TS guard remain disabled unless
   separately approved and proven; generic atomic RemoteSigner positive is a different
   contract. Independent reviews, focused Bitrue+Binance, strict/fmt/ratchets, fresh
   full CI and family WASM size/replay evidence precede any assigned guarded merge.

Architecture gate: no per-venue fix of a cross-cutting concern. Shared provider,
Selection, clock, lifecycle, classifier, session, execution and history cores own
authority. The venue owns canonical bytes, codecs and native continuation data.
No local Context maker, completion wrapper, retry engine or signer provider.

## Independent plan review refinement

Source/adversarial and caller reviews verified native signing/selection/session
shape, with one retry-scope correction: immediate transport retries exclude
catalogues, malformed-list/UTF-8 failures and a second parsed signature refusal.
The five-physical-attempt observation applies only to eligible non-market chains.
Those exceptions and exact source anchors are corrected before test implementation.
Full-map review CLEAR confirms all20baseline fingerprints, source holds and the
unapproved-only candidate transition. No production source is authorized.

## HMAC owner review corrections

claude-4yjlw0 confirmed preimage, header order, selection and listen endpoints.
The two Medium corrections are explicit: receipt-floor custody covers transport
retries as well as signature refusals, and exact native retry exclusions remain.
Source anchors, absence of DELETE, local listen retirement, existing error mapping
and the keyless unsigned signature-refusal reissue are recorded. Registry recvWindow
normalization was independently rechecked at registry/src/lib.rs:1140-1144.
The overseer assigned the shared receipt-floor prerequisite to claude-m26ps4.
Its proposed StampFloor from SigningRelease and DispatchContext::stamp_after is
consumer-fit CLEAR in direction only; no reviewed source pin exists. The native
negative-to-zero initial stamp boundary remains an explicit shared-fit question.

## Executed original native checkpoint

The isolated dispatch_signer_adoption target compiles and runs against unchanged
production85e155faebb:25controls,8intended assertion REDs,17working GREENs,
0ignored. Five REDs observe native provider-use count0 where1 is required
(catalogue, bracket, setting, listen POST, keepalive PUT); two require fresh
signature/transport retry batches (0vs2/4); the eighth exposes a setting POST
replayed after HTTP500/-1022. Later receipt/operation/terminal assertions after
a failing count or replay check are prospective requirements, not executed
production proof. The independent injected provider signs the expected canonical
atomic work successfully. Initial wrong rustfmt path and a missingCommand.side
fixture field were corrected before execution; neither is semantic RED evidence.

Working controls include independent Python HMAC vectors (including absolute
listen POST/PUT), static/ordered headers, header-only recvWindow, validation and
keyless/keyed-public-only catalogue behavior, fixed/backward timestamps, genuine
streamed fills malformed-list/invalid-byte failures with an empty-list positive,
unknown-write terminal/unchanged Store after an already spent signature budget,
and both interleaved retry-budget witnesses. Transport->signature->remaining
transports proves signature retry cannot reset transport count; signature->
transport->second signature refusal proves transport cannot renew that budget.
The HTTP500 negative forbids the same setting POST/provider preparation while
allowing native reconciliation GETs. No local native lifecycle source changes.

Scoped strict all-feature Clippy, package fmt and15checker unit tests PASS.
The conservative scanner against the EXACT frozen85 baseline passes33occurrences/
20fingerprints; all production, existing tests, checkers and inventories remain
byte-identical to that baseline. The full bans command against moving origin
reports only incoming Bitget/OKX prunes missing from this intentionally frozen
base; no global full-bans PASS or exemption is claimed. Later actual integration
must preserve those incoming entries/prunes and run the normal checks again.

Source/adversarial, caller and full-map reviews are CLEAN after replacing the
fabricated-error-string fixture with genuine codec events and strengthening the
Unknown terminal, exact diagnostic and retry interleavings. HMAC owner review
of664d8ac0032 is CLEAR, with shared StampFloor source review still separate.

The receipt-floor prerequisite is owned by the credential-core lane. Proposed
StampFloor is minted only by genuine SigningRelease; stamp_after(floor) applies
max(0,stamp_ms(),previousReleasedStamp+1), with safe-integer overflow refusal.
None means max(0,stamp_ms()), preserving the native negative-to-zero boundary
without changing other families' stamp_ms(). The floor is metadata only, not
retry/owner/credential authority. Constructor/source freeze and actual landing
remain gates. Production stays held on #1993, that reviewed shared seam and the
existing shared-file and lifecycle/transition approvals. No new STAGING row,
raw REST fallback, inventory delta, pushed branch or migrated-family claim.

## Authorized production composition

The overseer authorized production on reviewed #2024 b062c3a5d03; this branch
plain-merged actual #2017/#2019 through e4059dc9738. Their LocalSetting source
and refusal controls remain intact. StampFloor is inherited, with no seam edits.
Core and independent caller fit review require preserving explicit legacy
account/catalogue/coalesced/session/trade custody: these requests keep their
recorded request funnel, immutable Slots/Selection and opaque dispatch signing.
No logical twenty-second root is imposed on their native multi-attempt chains.

The first genuine managed signed read is FetchMaxLeverage/Brackets. It admits
SelectedRead::command_root at the genuine callback once, carrying its immutable
selection and raw caller deadline through the entire chain. Each eligible send
or reissue retires the previous physical leg, then derives a new Read/Never
20,000ms child from that original root. Validate root/leg delivery before codec
publication; only root completes. No invented 100s lifetime, retry renewal,
method-based credential selection or background issuer is introduced.

Every staged reissue retains its original selection and receives its stamp floor
only from that attempt's genuine SigningRelease. One CodeTable gates native
signature refusal; unknown HTTP500/408 cannot authorize replay. Staged writes
remain explicitly without managed journal/ResendGrant authority; this custody
cutover does not claim P0 accepted-ID aggregation or zero legacy completion.
Exact actual staging/policy maps must be measured and granted before changing
any bootstrap row. Hosted family admission remains disabled.

Verification: preserve all original 25 assertions and their native interleavings;
fixture-only trusted setup changes are permitted for opaque Options. Add a
near-20s bracket failure/reissue witness, provider rejection and immutable selection
controls. Independently review source/custody, caller behavior and exact maps;
run scoped Bitrue/Binance, strict/fmt/checker gates, then full CI on the PR head.

## Production candidate and exact accounting

Source follows the reviewed production composition, on inherited #2024 plus
actual e4059dc9738. Bitrue Options now contains only opaque primary/read Identity
refs and public configuration. Registry trusted setup consumes raw compatibility
input; supplying raw material to build_with_signer refuses. One dispatch recipe
presents the selected API key and signs the exact native preimage atomically.
Hosted family compatibility stays disabled. The native socket implementation,
listen-token lifecycle, Store projections, native callback ordering and budgets
remain; signed transport uses explicit shared Selection egress/evidence.

The original integrated checkpoint reproduced 17 GREEN / 8 assertion REDs. All
25 original assertions now pass with only trusted fixture construction changed.
Additional controls cover a second physical page finishing after 39,998ms under
the original caller root, strict caller expiry before Store projection, six
closed provider-result/refusal variants, and delayed atomic catalogue signing.
An old history test that fabricated engine error strings is replaced by genuine
invalid-byte/missing-list HTTP stream controls; native assertions remain strict.
Legacy test raw Options exist only in test trusted setup. Their erasure test is
fixture evidence, while native Options absence is pinned by a compile-fail doc.

Exact request inventory on actual e405: 29 occurrences / 17 fingerprints becomes
21 / 14. No STAGING_BOOTSTRAPS or SIGNER_POLICY_BOOTSTRAPS row is added or edited:
the original cx.request(method,url,headers,body,pending) expression remains exact
34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc x1.
Selection::sign is the existing shared coupling for that explicitly staged send;
it is not a new local signing wrapper or a claim that legacy ownership is managed.

The genuine completion conversion removes only these account.rs fingerprints:
187ae7a49b233001f96c827642f4839ad9e3ea4fd81b9b62848d3900055afced x1,
1b3029882eb1af7175e494797938adedfb4b3c6cbd38573c98c2e4940415d89b x1,
985d5251ce2c2f3e0d27d6c46def63a68db127135e27d9e2917a1923e354a48f x1.
Five conservative pending.request accesses disappear with the obsolete timestamp
allocator/resend string ladder: 8d7195dd58da632e34e4dc75d236e49b6d1b89cc10c243ec7e670ec59b511d26
x10 becomes x5. These five are accounting cleanup, not five additional migrated
commands. All other Bitrue request fingerprints remain exact. Other families'
manifest maps and ordering are preserved.

The signing ratchet now scans Bitrue sign modules; production key/secret holders
and the inline MAC are gone. Remaining Bitrue matches are the provider work
constructor, the unchanged diagnostic text, and explicit test-only vectors/setup.
No source rename is credited as authority adoption. The shared CodeTable gates
signature resend with native parsed numeric evidence and shared status precedence;
500/408 never replay the uncertain POST. Typed transport and stream callbacks
choose native continuation without parsing engine-formatted diagnostic strings.
Existing legacy terminal error/Partial ladders remain debt for the later genuine
Mutation owner/journal conversion; rows #21/#22 and accepted-ID aggregation are
not claimed closed by this signer cutover.

## Independent source review corrections

The first source pin727f had one stale accounting assertion: its extra floor
field access meant22/14, while the prose said21/14. Follow-up7dcba separates
stamp-floor metadata from the native prior-attempt flags and makes actual source,
scanner and manifest21/14. This is not a new migration claim.

Caller and source review found noncanonical code strings accepted by parse::<i64>
(e.g. "-01022"), and an Inspect GET receipt incorrectly promoting a later provider
refusal to Unknown without any write. Committed compiling controls a1459ae75d1
reproduced three assertion REDs with31working controls before correction. Native
code extraction now uses shared classify::decode_int; canonical numeric/string
-1022 positives remain. Provider failure uncertainty follows actual mutating
Effect plus retained native plan effects/accepted IDs, never StampFloor metadata.
An Inspect-only chain stays NotSent; a previously released mutation stays Unknown.

Full-map review also restored the existing history witness: a valid execution ROW
is staged before invalid bytes, with no interim publication/retry, then one strict
failure with no Partial. Missing-list is a separate real stream case. No synthetic
HttpFailed error string supplies classification evidence. Exact incoming5653-path
and all25original-assertion preservation review was independently completed on727;
final bounded delta review and CI remain separate gates.

Second bounded review found that feeding already decoded Text into decode_int
could strip a second quote layer. New committed1d3d0b313a7 POST/streamed-GET
controls reproduced two assertion REDs; code is now extracted as RawValue and
its exact raw JSON token is handed once to the shared strict decoder. Canonical
integer/string -1022 remains positive; nested quotes, padding and float/exponent
aliases never authorize reissue. This adopts shared canonical native-code policy.

The temporary ffa175 effect check also introduced one extra conservative matches!
occurrence. Provider-failure projection now consumes the same existing typed
Request::credential_role declaration used for signing, eliminating that duplicate
read/write classification rather than renaming bindings or adding an inventory
exception. Independent semantic-dedup review is required; current scanner and
manifest agree21/14. No financial authority is inferred from the role or floor.

Verification checkpoint: source/caller/full-map independent reviews are CLEAR at
2024ec88acc after all fixes; no remaining High/Medium. The map reviewer explicitly
confirmed typed-role reuse is semantic deduplication, with no hidden/renamed sites,
21/14 exact source/manifest and original25assertions unchanged. All 5,621 unowned
entries from the 5,653-entry incoming tree were preserved. Executed220Bitrue native
controls plus1compile-fail doc, registry3, TS construction4, and earlier305Binance
controls on identical engine source PASS.
Focused strict/checker/fmt gates pass; full fresh CI and HMAC-owner source review
are still pending. No WASM/replay/size result is inferred from native tests.

### CI follow-up: isolated family provider bootstrap

The isolated Bitrue WASM replay failed before startup: the registry feature
installed dispatch signing, but the WASM host's Bitrue feature omitted its own
`dispatch-signing` gate. Combined/native feature unification hid that omission.
Add that existing host feature to the Bitrue row only; keep the constructor and
provider custody unchanged. Use the failed family replay as the original RED,
compare the standalone Cargo feature closure, run scoped strict checks and let
fresh family/replay CI prove the corrected bootstrap. The CPU-plan and replay
failures downstream of the missing measurement are not independent timing misses.

The secret scanner also flagged historical plan prose at commit 48f8b6cbe57,
line 271 (the API-fit attribution sentence), not signing material. Reword the
current sentence. An exact historical fingerprint allowance requires the
maintainer's explicit approval; no broad rule/path exemption or history rewrite.
