# Kraken whole-family socket lifecycle

Owner: codex-9czdjh. Reviewer: codex-uufub4 (socket/nonce custody); shared API
changes additionally need the engine owner. Baseline: actual
`49a5b54f36232d1c926f4280141d92060db18e2b`. Plan only: no source, feature, inventory,
hosted activation or signing cutover. Native-drain #2171 and NX #8172 establish
separate retirement prerequisites, not socket or nonce takeover authority.

## Scope and exact inventory

Move Futures public/private and Spot public/private to the existing managed
feed/epoch/interest lifecycle together. The first registered feed enables the
engine-wide managed gate; leaving a legacy sibling unregistered would break its
connect/send/ingress. Register both real feeds for each venue before either can
connect. Preserve native startup eligibility and PublicFeedPolicy suppression;
public-only accounts start no private socket, auth, signing or token request.

The socket inventory counts references, not nine independent connections:
four physical connect sites plus five calls to the legacy public connect helper.
The exact six path/fingerprint rows total nine occurrences:

| Path relative to sdk-rs | Fingerprint | Count | Meaning |
| --- | --- | ---: | --- |
| `venues/kraken/src/account.rs` | `297733c7eb42dfbca2cbf5172bc2d8d63e74f1cff9c223adc1a9bff1c7b526ed` | 1 | physical Cx::connect site |
| `venues/kraken/src/lib.rs` | `4de7e8e27dd8db2b87ec30fe4c77928d419debb784462b2e420932a1076af845` | 1 | physical Cx::connect site |
| `venues/kraken/src/lib.rs` | `974264f5bfd3c268b4c30755e61c8269f51a1cd01ebaf1e6a51c77a8e58659cb` | 2 | legacy public connect-helper references |
| `venues/kraken/src/spot/account.rs` | `fa61c8e381d26f7e9dc72cec5abd5a874645bf5f1cec71af7ffa68e4babba0d6` | 1 | physical Cx::connect site |
| `venues/kraken/src/spot/mod.rs` | `974264f5bfd3c268b4c30755e61c8269f51a1cd01ebaf1e6a51c77a8e58659cb` | 3 | legacy public connect-helper references |
| `venues/kraken/src/spot/mod.rs` | `d1fb86f0d751998134c0de52dfcade47e3341e511b77d521ff9083aa447f7e21` | 1 | physical Cx::connect site |

Target: all nine references disappear through real lifecycle ownership, and the
Kraken family leaves socket-legacy-families.json. Delete the superseded helper
and its manual socket/open/Supervisor state; do not rename a receiver to hide a
legacy call. Request, signing, credential and namespace inventories remain
unchanged. No checker exception or unknown-socket exemption.

## Native policy to preserve

All times below are milliseconds; inherited values are from socket::Config.
All four feeds retain 1,000 base backoff and 30,000 stable threshold, jitter and
traffic-based reset, not a reset from opening alone.

| Feed | Endpoint | Connect / idle / ping | Backoff cap | Native auth/readiness |
| --- | --- | --- | --- | --- |
| Futures public | futures.kraken.com/ws/v1; demo-futures.kraken.com/ws/v1 on testnet | 45,000 / 45,000 / 15,000 | 16,000 | No auth; native public interest replay on open. Current open text refreshes receive before decoding. |
| Futures private | Same configured Futures WS URL | 15,000 / 60,000 / 10,000 | 30,000 | Challenge wait OPEN+15,000; one accepted nonempty challenge renews ACK wait to receipt+15,000. All three distinct subscribed ACKs end auth. Only successfully decoded native Ws envelopes refresh receive. |
| Spot public | ws.kraken.com/v2 | 45,000 / 45,000 / 15,000 | 16,000 | No auth. Native ticker/book/OHLC replay; current text refreshes receive, including malformed text. |
| Spot private | ws-auth.kraken.com/v2 | 45,000 / 45,000 / 15,000 | 16,000 | On open, signed HTTP GetWebSocketsToken then executions/balances subscriptions. No separate native auth ACK deadline or wait-for-ACK publication gate. Current text refreshes receive before decoding. |

Keep exact native URL schemes, testnet selection, channel roles, wire JSON and
ordering. Futures ping is {event:ping}; Spot ping is {method:ping}. Do not
introduce a private first-pong rule or an auth gate for Spot.

Futures private also requires initial payloads for fills, open_orders and
open_positions by OPEN+180,000. Native valid empty snapshots count; a heartbeat,
ACK or malformed payload does not. This is distinct from authentication and
idle. Once all initial payloads are accepted, a quiet account with native ping
traffic can remain connected indefinitely. No age-based private resubscription.

## Ownership and codec mapping

1. Keep engine-issued FeedId/Epoch references and native codec/account data.
   The engine owns physical identity, open/auth phase, expiry, reconnect grant,
   queued-frame fencing and interest revision. Replace manual timer/retry
   routing; never retain a second venue epoch counter or Supervisor.
2. Public feeds use AuthMode::None and transactional current interests. Preserve
   Futures ticker/book batching and product selection; Spot symbol translation,
   ticker trades trigger, depth100 and supported OHLC interval deduplication.
   Reconnect replays the current interest set once, including edits made while
   disconnected. Duplicate open/ACK and obsolete same-epoch batches grant no
   second replay. Retire the exact feed before book invalidation and retry;
   preserve native book/sequence/checksum/synced resets and book_changed even for
   empty books. Private retirement never clears public depth or financial state.
3. Futures private uses native challenge/ACK codecs with managed Required auth.
   OPEN sends exactly the Primary key-only challenge frame. One valid current
   challenge yields the landed AuthEvidence::Progress, not Accepted. Its native
   callback retains the challenge and emits the three signed subscription frames
   in order: fills, open_orders, open_positions. Duplicate/empty challenges do
   nothing. Decode distinct subscribed ACKs; only completion of the full set
   yields Accepted. No unrelated success or data frame is auth proof. Native
   financial payloads after challenge but before all ACKs remain admissible;
   managed Authenticating ingress already permits native Other messages.
4. Keep Futures challenge HMAC and the current credential/signing path byte-exact
   in this lifecycle cut. The landed challenge_work recipe is pure work, not a
   socket-epoch or remote-provider adoption. No signing row is pruned and no new
   Sign callback or digest channel is introduced. Challenge frames and signatures
   are epoch-owned one-shot work, never persistent replay interests. Existing
   rejection classes still reconnect with their native diagnostics; no new
   account latch, retry or permanent startup refusal.
5. Spot private maps to AuthMode::None for lifecycle readiness, with native error
   decoding and AnyText traffic. Open starts the existing token HTTP request,
   bound in Pending to the exact engine-issued Epoch. Adopt its response only
   while that same epoch is current/open; a token for an old socket cannot send
   on a replacement. Keep the exact two subscription JSON shapes and order
   (executions then balances), including snapshot flags. The token is consumed
   only at that epoch's WS boundary, not stored as an interest or logged. Closing
   retires its eligibility immediately. HTTP nonce/signing/transport behavior
   stays native; this is not private dispatch adoption or a new bearer Session.
6. Retired/foreign/unopened messages, binary frames, opens, closes, timers and
   late token/signing completions cannot mutate health, Store, auth, notices or
   reconnect state. At expiry equality, ingress cannot rescue a feed before its
   delayed timer arrives. Disposal stops all registered feeds, clears epoch-owned
   private auth state and queued work, and cannot reconnect. A resource/ID refusal
   leaves no partial registration, frames or interest credits.

## Shared API dispositions required before source

Overseer ruling: native parity is mandatory for all three cases below. Managed
sockets owner claude-55ao7t decides which are already expressible by the engine;
for any gap, that owner names the smallest shared change and the overseer assigns
it separately. No venue-local Supervisor, watchdog or policy approximation.

AuthEvidence::Progress is already present at the baseline; do not rebuild that
seam. Two native timing/traffic requirements do not fit a blind policy mapping:

- Futures private receive refresh follows successful decoding of its exact Ws
  envelope. Current TrafficPolicy offers AnyText, PongOnly and TextAfterFirstPong;
  none means that native predicate. AnyText would let malformed text extend the
  60-second bound, while treating all valid data as Pong would consume it before
  the financial reducer. Request an engine-owned typed codec traffic decision,
  or an explicit owner disposition, before source. No venue receive-clock copy.
- The 180-second initial-payload bound starts at OPEN and is independent of the
  15-second challenge/ACK auth bound. Existing register_socket_feed_with_snapshot
  uses CONNECT origin and the same auth_timeout_ms, so it cannot be substituted.
  Request a narrow engine-owned independent native snapshot bound/completion
  using admitted message evidence. Native codec retains which payloads validated;
  the shared owner retains time, expiry and once-only discharge. No fake snapshot
  completion, renewed bound on ACK/Progress, or parallel venue watchdog.

Also pin stable-reset anchoring: native Futures supervision becomes Ready at
OPEN, while managed Required auth's authenticated transition resets Supervisor's
since at final ACK. Preserve native OPEN-based 30-second stability/reset behavior; the managed-sockets
owner identifies the existing expression or smallest shared change. Auth latency
must not extend that anchor.
These are review/source gates, not approved new APIs. No generic implementation
belongs in this docs checkpoint. Any required shared seam gets its own plan,
originals and owner review before the whole-family activation.

## Originals after plan/API direction CLEAR

Use real Engine startup, physical IDs, issued epochs and actual frame drain;
retain every native financial assertion and the full socket_supervision target.
Native compatibility positives and demonstrated new-contract REDs stay separate.

- Four-feed healthy startup/reconnect, Direct versus suppressed public feeds,
  public-only accounts, sibling isolation and exact native subscription/ping bytes.
- Unopened, retired and unknown open/message/binary/close/timer ingress: no Store,
  notices, health, replay or new physical work. Dispose followed by late callbacks.
- Repeated flap/cap/jitter/stable reset, delayed open and delayed auth with the
  original stable anchor. Native connect and idle equality with delayed timers.
- Futures challenge at 14,999 versus 15,000; accepted challenge renews ACK wait
  once; duplicate/empty/wrong-epoch challenge cannot renew it. Wrong, duplicate
  and missing feed ACKs do not complete auth. All native rejection classes.
- Futures exact challenge MAC and three signed subscription bytes; pre-final-ACK
  financial payloads remain native. No new signing capability or secondary login.
- Successful native decode versus malformed JSON/wrong-type Ws envelope near
  idle expiry; traffic cannot renew the independent auth or initial-payload bound.
- Missing one of the three initial payloads at OPEN+180,000, valid empty snapshots,
  malformed snapshots, quiet authenticated accounts, and duplicate completion.
- Spot token held through close/replacement/dispose and stale success/failure;
  neither sends or retires a new epoch. Fresh reconnect fetches a fresh token.
  Positive native behavior does not wait for a subscription ACK.
- Books clear before replacement, refcount/interests edited while disconnected,
  OHLC deduplication, old queued batches, and full capacity release/recovery.
- NaN/infinity/equality ingress and allocation failure: typed refusal, no partial
  frames/publication, preserved native diagnostics and exact cleanup custody.

## Qualification and publication sequence

Plan direction review by uufub4 first. Then native assertion originals, exact
shared API dispositions, and source only after those holds are resolved. Keep
socket lifecycle separate from the native REST/nonce takeover work and the
working hosted adapter. Never enable a bridge that breaks a live account or
falls back from remote signing. #2126 remains HELD.

Before a source PR qualifies, merge the actual origin/feat/rust-rewrite tip with
plain merges. Check Kraken native/socket/signing/Spot/public and the affected
managed-engine/host controls, strict scoped Clippy, fmt and full shrinking-map
bans. Remove only the nine socket occurrences and Kraken's temporary family row;
request/signing/credential inventories remain exact. Publish the caller-visible
managed expiry/resource/retirement differences in CALLERS, without implying a
nonce takeover, remote nonexecution or journal rewrite.

CI records Kraken family sizes. Measure one local composed combined on the
current landing tip before READY; keep a paired Kraken/combined comparison for
the newly linked managed lifecycle. If generic engine/provider code is required,
measure Bybit and Bitget on that same composed tree too. No budget/profile/gate
changes. Delete each own scratch target after measurement, keep sources/logs and
the size table, and run no competing size builds in this lane. Source CLEAR,
qualified green CI and auditor exact-tree GO precede guarded SDK landing. This
plan records source inspection only; no originals, builds or execution claimed.
