# Native exchange server

The Rust replacement for NX `services/exchange-server`. It goes live on Vultr
before the hard cutoff at 2026-11-02 00:00 UTC.

The design is a hybrid. It keeps today's `wss /account` JSON envelope, so the
web hosted client changes only how it decodes store data. Each account runs on
the native `FrameServer` engine actor (`hosts/native/src/server.rs`). Store data
travels as SCXF frames inside `msg`. Trading uses the existing `callMethod` op
names.

Owners. Comment in your own section, and don't edit anyone else's.

| Area | Owner | Where |
|---|---|---|
| Server binary, envelope, sessions, accounts, HTTP | claude-fe7lgm | `hosts/server` (crate `safe-cex-exchange-server`) |
| FrameServer runtime account API | claude-fe7lgm | `hosts/native/src/server.rs` |
| Remote signer (sign-req / sign-res provider) | claude-4yjlw0 | `hosts/server/src/signer.rs` |
| Routing plan resolution | claude-m26ps4 | `safe_cex_native::routing::resolve` |
| Web hosted client (frame decode) | claude-19pt8s | NX `hostedFrameClient` / `ExchangeServerWorker.ts` |
| Docker images and GHCR workflows | codex-mxb4mq | SDK Dockerfiles, NX workflows |
| Candle server | see its own section | `hosts/candle-server` |

## Packaging (pinned for codex-mxb4mq)

- **Crate:** `safe-cex-exchange-server`, at `sdk-rs/hosts/server`. It is a workspace member, but not a default member.
- **Binary:** `exchange-server`. Build it with `cargo build --release --locked -p safe-cex-exchange-server --bin exchange-server`.
- **Listen:** `0.0.0.0:${TEALSTREET_PORT:-23456}`. HTTP and WebSocket share this one port.
- **Readiness and liveness:** `GET /health`.
  - Returns 200 with `{"ok":true,...}` when ready, and 503 with `{"ok":false,...}` otherwise.
  - Healthcheck: `curl -fsS http://127.0.0.1:${TEALSTREET_PORT:-23456}/health`.
  - It goes 200 once the listener is bound and the account threads are running. No venue I/O is needed for it.
- **Runtime files:** none.
  - TLS uses rustls with the bundled webpki roots, and the edge terminates client TLS.
  - CA certs in the image are harmless but not required.
- **Signals:** SIGTERM starts a graceful drain. Clients get a `1001` close, and accounts are disposed. The process exits within 10s.
- **Logs:** JSON lines on stdout. Warn and above go to stderr as well.

### Image contract (codex-mxb4mq)

The SDK Dockerfiles build from the repository root and install native binaries
only into a Debian slim runtime running as UID/GID 65532. Exchange publishes as
`ghcr.io/tealstreet/exchange-server-native`, port 23456; candle publishes as
`ghcr.io/tealstreet/candle-server-native`, port 23457. Both use `GET /health`.
Candle's `candle-cache-owner` is installed beside `candle-server` after a separate
Cargo invocation, preserving engine/cache JSON feature isolation.

This mirror's `docker-exchange-server-native.yml` and
`docker-candle-server-native.yml` build on push to `master` only, publish
`sdk-<mirror sha>` to GHCR, and keylessly sign the exact pushed digest. Nothing
publishes `latest`; the Vultr boxes run digest pins. They require GitHub OIDC
issuer `https://token.actions.githubusercontent.com` and the exact certificate
identity of this mirror's workflow (the monorepo's `*-server-rust.yml` signers
were deleted 2026-11-07):

- Exchange: `https://github.com/Tealstreet/tealstreet-sdk/.github/workflows/docker-exchange-server-native.yml@refs/heads/master`
- Candle: `https://github.com/Tealstreet/tealstreet-sdk/.github/workflows/docker-candle-server-native.yml@refs/heads/master`

Images do not contain credentials and the workflows do not deploy hosts.

## Environment

| Variable | Default | Meaning |
|---|---|---|
| `TEALSTREET_PORT` | `23456` | Listen port |
| `TEALSTREET_BOX` | unset | Stable display / Sentry box label, e.g. `exchange-01`; never router ownership identity |
| `TEALSTREET_ROUTER_BOX_ID` | unset | Opaque canonical ID from router `BOXES`, e.g. `01`; required to enable enforcement, exact heartbeat `box` match |
| `TEALSTREET_ROUTER_LEASE_MS` | `90000` | Full lease in milliseconds (20000–3600000); asserted ownership may extend at most half this duration |
| `TEALSTREET_ROUTER_LEASE_ENFORCE` | `0` | `0` records shadow ownership only; `1` enforces expiry and parks algos, with missing users fenced immediately unless an explicit enable grace is active |
| `TEALSTREET_ROUTER_LEASE_ENABLE_GRACE_UNTIL` | unset | Optional absolute Unix-millisecond deadline for the first enforcement flip only, typically now + 90000; unset or past grants no grace, and restart never extends the deadline |
| `TEALSTREET_ACCESS_TEAM_DOMAIN` | unset | Bare `<team>.cloudflareaccess.com` host for release-proof JWT verification; no scheme, port or path |
| `TEALSTREET_ACCESS_AUD` | unset | Hexadecimal Access application AUD for `/proof/*`; either missing/invalid value disables Access-authenticated proof, heartbeat and interruption-header handling and logs once at startup |
| `TEALSTREET_IDLE_RELEASE_TTL_MS` | `20000` | Idle-release lease in milliseconds; operator drains never expire |
| `CONNECT_REGION` | unset | Region label. Routing uses it for `proxyRegion` defaults; it also appears in `/health` and logs |
| `TEALSTREET_RELEASE` | unset | Image revision shown in `/health` and as the Sentry `release`; whitespace and placeholder values (`undefined`, `null`, `unknown`) are omitted |
| `TEALSTREET_BOX` | unset | Stable Sentry `server_name` and `box` tag (for example `exchange-01`); no hostname fallback |
| `EXCHANGE_SERVER_SENTRY_DSN` | unset | Native error capture (`src/telemetry.rs`); Sentry is off when unset |
| `REDIS_URL` | unset | Optional. Not used in v1; it is accepted so that today's compose files keep working |
| `TEALSTREET_ENGINE_THREADS` | number of CPUs | Account actor threads (see Runtime) |
| `TEALSTREET_DIAGNOSTIC_BUILD` | `0` | `1` enables `diagnostic:server-health` frames for verbose clients |
| `TEALSTREET_DELEGATED_ORDER_FRAMES` | `0` | Startup opt-in for hosted venues' `order.*` frames: `0` disables, `1` enables only with compiled protocol support and hosted signing/origin support; other nonempty values refuse startup |
| `EXCHANGE_ALERT_SLACK_WEBHOOK_URL`, `EXCHANGE_ALERT_CRITICAL_SLACK_WEBHOOK_URL` | unset | Deferred past cutover. Accepted and ignored |

The production image compiles the `delegated-order-frames` feature but keeps its
runtime switch off. Change the container environment and drain/restart the same
image to change activation; this is not a hot-reload control. A restart drops
sockets and memory-only delegated copies, so accounts require fresh admission
and handover. Existing LOCAL/COSIGNED calls and the delegated legacy method path
are independent of this order-frame switch.

`GET /capabilities` and `/account/capabilities` expose
`transports.accountWorkerWebSocket.delegatedOrderFramesV1` from that same runtime
control. The box never advertises `delegatedOrderLedgerV1`; only the router owns
that capability. Hosts must discover both per connection before passing a ticket
to the outbox. See [order-frame activation](../../../docs/plans/callers/delegated-order-frames.md#rolling-activation)
for deployment ordering and rollback.

`delegatedActivationScopeV1` advertises the partial action set allowed by Sam on
2026-11-11: place and reduce-only close; amend, venue cancel, cancel-all and nuke
are false and not activation prerequisites. Hosts query `?exchangeName=<joined venue>` on either capabilities route.
The action scope is computed for that venue, not a representative venue.
`delegatedVenueV1` echoes the selection; missing, unsupported or unhosted
selections advertise no actions. SDK activation checks that the echoed venue
matches its admitted connection. The per-user `DELEGATED` flag
belongs to the admitting host; there is no same-named server environment flag.
The existing runtime frame switch and router-ledger proof remain necessary.
Legacy unsupported DELEGATED method calls return `DelegatedActionUnavailable`
before native handoff. LOCAL/COSIGNED retain their existing action translation.

`TEALSTREET_WORKERS`, `TEALSTREET_GATEWAY_PORT` and `TEALSTREET_HEALTH_PORT`
belong to the Bun gateway and have no meaning here. One process serves all
connections.

The native binary writes JSON logs and, when `EXCHANGE_SERVER_SENTRY_DSN` is
set, sends Sentry events (`src/telemetry.rs`). Its identity rules match
`packages/service-telemetry`, which the Bun gateway and exchange workers use.
The release is the runtime `TEALSTREET_RELEASE` image revision, omitted when
unset, and never the literal `undefined`. Image OCI revision labels are
external metadata, not process environment variables. Box identity comes only
from `TEALSTREET_BOX`; missing or invalid labels are omitted. Events omit
`region` because `CONNECT_REGION` can be a legacy routing default rather than
the box location. The SDK infers no host, release or environment.

What it captures:
- panics, through a panic hook that captures without flushing (a caught panic
  on an engine thread never waits on Sentry), sending the code location and
  never the payload;
- `Log::error` conditions: drain and break-glass deadlines, router-lease
  fencing, a failing signer, and algo shutdown deadlines;
- the native host's account start failures and venue, relay and HTTP-tunnel
  transport failures, through `safe_cex_native::report`. Each account's
  transport is built `with_venue`, so its failures carry `venue` beside `kind`
  and `target` (host and route, never a URL). The shared HTTP-tunnel socket
  pool serves many venues and reports none.

Warnings are not sent.

Every event is rebuilt from an allowlist:
- a fixed summary per condition, never error text or a venue payload;
- a scrubbed fingerprint and exception type;
- stack frames' function, module, file, line and column only;
- tags `service`, `box`, `kind`, `error_kind`, `venue_code`, `venue` and
  `target`;
- extras `user_tag` (a hash, never the user ID) and
  `suppressed_since_last_event`.

Account names, URLs with their query, auth headers, key-shaped strings and
payloads do not leave. Events are throttled to one per fingerprint per 60 s
and 30 per minute overall, tracking at most 500 fingerprints. They are sent
from Sentry's transport thread through a bounded queue that drops when full.
The only flush is the guard's at shutdown, bounded to 2 s, so telemetry never
blocks or fails a trading path.

## HTTP routes

- **`GET /health`:** `{ok, uptime, release, region, connections, accounts, engineThreads, remoteSigner}`. The last field carries aggregates only: no account, symbol or key ever appears here.
- **`GET /capabilities` and `GET /account/capabilities`:** the same JSON as `getExchangeServerRuntimeCapabilities()`, with `"runtime":"rust"` added.
- **`OPTIONS`** on the three routes above returns 204 with the existing public CORS headers.
- **WebSocket upgrade:** accepted on `/` and on `/account`.
  - Query parameters: `proxyToken` (alias `token`), `resume`, `resumable=1`.
  - Without a token, the socket is an unauthenticated public-data socket.
  - An invalid or expired token gets a 401 before the upgrade.
- **Any other request:** 426 if it is not a WebSocket, otherwise 404.

## Drain and roll

A box is drained before a restart, so no Hosted session is dropped and no
chase or TWAP is cut short. `scripts/roll.sh` runs the whole roll on the box.

- **Admin routes:** `POST /admin/drain`, `POST /admin/drain?now=1` (break-glass) and `POST /admin/undrain`.
  - They answer only a loopback peer whose request carries no proxy header (`cf-connecting-ip`, `cf-ray`, `cdn-loop`, `x-forwarded-for`, `x-real-ip`, `forwarded`); anything else gets 403. cloudflared dials the port from loopback, so the peer alone proves nothing.
  - Each answers `{drain: "off"|"waiting"|"now", algos: {chases, twaps}}`; break-glass adds `settled`.
- **`/health` while draining:** 503 with `ok:false` and `draining:true`, so the load balancer pool fails over. `algos: {chases, twaps}` counts the chases and TWAPs on this box, parked accounts included. A stopped chase counts until its cleanup ends, since that may still need a signature; a paused TWAP counts too.
- **The moved close:** `4100` with reason `draining: reconnect`. It means this box is going away and the page should reconnect.
  - A join/re-snapshot racing an account deliberately stopped for an active drain may send the original `op:error` frame with optional `data[1].reason: "moving"`. This marker requires the native account handle's first disposal cause plus an active drain for its user or this box. Native disposal/termination and host cleanup share a one-time cause record; a later holder release cannot relabel an earlier stop or crash. Old clients still receive the same first error argument.
  - Break-glass disposal never marks a planned move; its `forceToast` algo notices are unchanged and still flush before 4100.
- **While waiting:** a socket whose accounts run no chase or TWAP gets the moved close when the drain starts, and again as each algo ends. A socket with a running algo stays until those algos end. The drain never stops an algo.
  - An `init` from a user with no account on this box gets the moved close. A user already here (a second tab, a resume) is still accepted.
  - A moved socket releases its accounts instead of parking them: its page is reconnecting elsewhere.
- **Break-glass:** sticky force intent is published before any cleanup await and excludes planned-move classification until explicit undrain. It stops every account through the SIGTERM path. Each live page gets the algo host's own stop notice ("Chase on X stopped: the server is restarting."), and the cancels settle while browsers can still sign. Only then does every socket get the moved close, with the stop notice flushed first.
- **`roll.sh [--now] [--ceiling SECONDS] [--unit NAME] [ADMIN_URL]`:** drains, then polls `/health` until no chase or TWAP runs. The ceiling defaults to 1800s; past it the script fails and leaves the box drained. It then runs `systemctl restart exchange-server`, waits up to 120s for `/health` ok, and undrains. `--now` uses break-glass instead of waiting.

### Per-user drain (psmp2r-1)

These routes move one user while `/health` and everyone else keep serving. All
admin requests below use POST and the existing on-box `admin_peer` check, with
no forwarded headers. JSON bodies are bounded to 2 KiB. Never send them through
the public router. Only the on-box inventory response contains raw user IDs;
new drain logs contain `user_tag` only.

| Path | JSON body | Result |
|---|---|---|
| `/admin/user-drain/start` | `{userId, capMs?}` | Install an admission latch and return the summary below. Default cap 1,800,000 ms; accepted range 1–86,400,000 ms. Repeating start returns the same drainId and original cap. |
| `/admin/user-drain/status` | `{userId}` | Current summary, including users with no socket and parked algos. Does not start a drain. |
| `/admin/user-drain/cancel` | `{userId, drainId}` | Remove that exact latch and return status. Stale/missing ID, an in-progress force, or sockets already committed to close returns 409. |
| `/admin/user-drain/force` | `{userId, drainId}` | Explicit break-glass for that user only; return summary plus `settled`. Stale/missing ID or another force in progress returns 409. |
| `/admin/users` | none | `{users:[{userId,userTag,liveSockets,hasRunningAlgo,runningJobs,drain}]}` sorted by userTag. `drain` is the same summary as status, or null. Includes idle parked accounts, parked jobs, live sockets and installed latches. More than 4096 inventory entries returns 503, never a partial roll inventory. |

The summary is `{userTag,drainId,state,capMs,elapsedMs,hasRunningAlgo,algosDone,
pendingStarts,runningJobs,liveSockets,released,closeCode:4100}`. `drainId` and
`capMs` are null when off. States are `off`, `waiting`, `capped`, `forcing`,
`closing`, `drained`. Each running job is `{accountName,kind:'chase'|'twap',id}`;
stopped chases still cleaning up and paused TWAPs count. `pendingStarts` also
conservatively counts outstanding EXEC continuations that may hand off an algo and TWAP slice placements whose row has already stopped.
`hasRunningAlgo` includes that pending work, even before it has a book row.

Starting the latch stops `chaser:start`, `twap:start`, `twap:resume`, `twap:state`
and `exec` immediately before actor dispatch. Queued but undispatched starts get
`ServerDraining`, `kind:venueUnavailable`, `outcome:notSent`; nothing is replayed.
Box drain uses the same admission refusal. Stop, pause, terminate, cleanup and
signing remain available. Nuke remains available while waiting/capped and is counted through settlement; once closing or force starts, a queued nuke is refused before dispatch. The dispatch fence includes synchronous job creation
and publication of its running-work snapshot, so admission cannot race an idle
proof. Already-running jobs continue, including through the resume grace.

Before the cap, observed completion of all the user's jobs and pending work
closes every one of that user's sockets with 4100, `draining: reconnect`.
An idle tab stays while another account or parked session of that user runs a
job. Closed sockets release rather than park their accounts. A latched user's
new sockets receive 4100 once jobs are done; they cannot restore schedules here.
The latch remains until explicitly canceled. Once closing is committed, cancel waits until all closing sockets have left (409 meanwhile); it cannot reopen admission under a pending 4100 handoff.

At the cap, `capped` keeps jobs, signing sockets and the admission latch intact.
It does not kill or move anything, even if completion is first observed later.
The caller must choose force or cancel. Force uses the existing algo stop,
notice and cancellation-settlement path, bounded to 25 seconds; disconnecting
its HTTP client does not abandon cleanup. Live pages receive stop notices
before 4100; parked pages retain them for the next init here. If work still
runs at the bound, `settled:false` reports it and leaves the latch in place;
there is no release proof. As with box break-glass, canceled/stopped algos can
leave uncertain resting venue orders: their notices tell the user to check.
`released` proves that no hosted algo remains, not that every venue order was
canceled or filled.

### Live release proof and placement handshake

`POST /proof/user-drain {userId,drainId}` is read-only and returns exactly
`{userTag,drainId,released:boolean,checkedAt}` (`checkedAt` is Unix milliseconds).
It never returns job lists, account names or the raw user ID. Unknown or stale
IDs return 200 with `released:false`. True requires the same installed latch,
`algosDone`, zero `pendingStarts`, and zero live sockets. An unlatched idle
observation or a 4100 close alone is not placement authority.

The edge must require the Cloudflare Access service-token policy. Independently,
the box verifies `Cf-Access-Jwt-Assertion`: RS256 signature against
`https://<TEALSTREET_ACCESS_TEAM_DOMAIN>/cdn-cgi/access/certs`, exact issuer
`https://<host>`, the configured AUD in `aud`, and unexpired `exp` (no leeway).
It checks `nbf` when present. Team config accepts only a bare single-label team
host under `cloudflareaccess.com`; URLs, paths, userinfo and ports are refused.
JWKS has a five-minute TTL, a 256 KiB/64-key bound, a two-second HTTPS deadline,
no redirects and a single-flight refresh; unknown-kid refresh is limited to
once per 30 seconds. An expired or unavailable key set fails closed. Invalid
JWTs get 403 without logging tokens. Proof requests allow a burst of 40 then
10/second per direct peer IP, with at most 1024 active source entries; all calls
through one cloudflared peer share that limit. Responses have `Cache-Control:
no-store` and no CORS. The public router must 404 both `/proof/*` and `/admin/*`.

Roll/rebalance sequence: start locally, retain drainId, poll until released,
then let the router obtain a fresh Access-authenticated proof from the source
box immediately before pin/apply. If capped, explicitly force or cancel. A
normal 1006 close cannot authorize moving parked jobs. Keep the source latch
installed throughout the placement decision; do not race cancel against apply.
After destination ownership is confirmed, the router's same-owner reconnect
path needs no transfer proof. The NX consumer is PR #8936. No endpoint transfers
algo state or replays writes, and no server deployment is performed by this PR.

### Lazy owner moves and idle-release lease

Rebalancing is lazy: move on reconnect, unless capacity or urgency requires an
operator move. Every move uses the same admission gate and job-release decision;
urgency does not itself stop, park or cancel work. Only the existing explicit
force/settlement path may stop algos. Idle release only observes and fences.

`POST /proof/user-idle-release {userId,requestId}` uses the same fail-closed
Access verifier, request limits and no-store/no-CORS policy as `/proof/user-drain`.
The public router must return 404 for this path too. `requestId` is a
router-generated UUID per attempt. Under `User::state`, the same lock held by
`User::admit` through dispatch and `Running::refresh`, idle release checks jobs,
pending starts and live/pending sockets and installs the latch atomically.
An authenticated upgrade reserves a socket under this lock before returning 101;
it cannot slip between the idle check and latch installation.

Success is 200 `{userTag,drainId,released:true,checkedAt,expiresAt}`. Refusal is 409
`{released:false,reason,checkedAt}` and changes no admission or job state. The
closed reasons are `live-socket`, `running-jobs`, `parked-jobs`, `pending-starts`,
`already-draining` and `request-spent`. Paused TWAPs and jobs held by a parked
session still prevent release. Positive work on a revoked lease or already lease-parked account
returns `parked-jobs`, including pending work before its actor publishes paused
rows. The idle-release route never invokes the park or cancel path. Its on-box
admission reservation is acquired before interruption-header authentication and
before HTTP 101, then held until `Session` owns the socket count, without a gap.
No job is stopped, parked, canceled or replayed by this route.

Both timestamps are Unix wall-clock milliseconds, captured once with
`expiresAt = checkedAt + TEALSTREET_IDLE_RELEASE_TTL_MS`. A repeat of the same
`(userId,requestId)` while the idle latch lives returns the exact original proof,
including both timestamps; another requestId gets `already-draining`. It never
renews the lease. Expiry, cancel and operator takeover retain a requestId
tombstone for at least twice that attempt's original TTL after retirement.
A spent request returns 409 `request-spent`, with no new latch. History survives
idle-user pruning. Each user has 64 history slots, reserved at issuance; protected
IDs are never evicted to admit another attempt. A new attempt at capacity returns
429 without latching; live retries and spent-ID refusals still work. Expired
history is reclaimed, without refreshing its retention on a lookup.

The router accepts only a checkedAt within 5 seconds of its clock and an
expiresAt at least 5 seconds ahead. It must durably commit the new
owner before expiresAt minus 5 seconds; otherwise abandon the move and stay on
the owner. On an uncertain timeout, retry once with the same requestId to recover
the proof, then give up. Expiry bounds an orphaned latch to 20 seconds by default.

An expiry timer clears only its exact idle-release drainId, only with no live or
pending sockets. Admission also checks expiry, using the monotonic deadline as
well as wall time, so a clock rollback cannot extend a lease. A newer latch or
an operator drain is never cleared by that timer. Explicit admin cancel with the
same drainId still works after destination ownership commits; the router does
not require admin access. An operator start/force takes over an idle latch as a
permanent operator drain; ordinary operator drain behavior is unchanged.

## Router ownership lease and self-fencing

The frozen router/box interface is:

> Ownership is asserted per user by each owning user DO's alarm (every <=10 s, including parked users); there's no directory. POST /lease/router-heartbeat {box, issuedAt, users:[{userId, ownerEpoch, sequence, notAfter}]}, bounded batches, with the same Access service token + on-box JWT verification as /proof (public 404). Box rules: verify the authenticated box identity matches; reject stale or future issuedAt beyond the skew bound; per-user monotonic (ownerEpoch, sequence), so an older epoch/sequence never supersedes a newer accepted one; NEVER extend a user's lease past their notAfter; fence the user when now > notAfter (the box-side lease is notAfter itself, not a box-computed N/2); missing or uncertain state fails closed (fenced). Router side (ih35js, later): persist ownerEpoch/sequence and the max notAfter ever issued BEFORE sending; takeover first retires the sender epoch and stops old alarms, then waits past the max issued notAfter plus the skew margin, never trusting the last ACK. Constants I'm setting: clock-skew bound 5 s; notAfter = issuedAt + 45 s (TEALSTREET_ROUTER_LEASE_MS default 90000 means takeover is no earlier than ~50 s after the last issued heartbeat); heartbeat cadence 10 s. Unfencing needs a fresh valid assertion naming that user with a newer (epoch, sequence).

Contact means a valid authenticated assertion for that user, including users whose
sessions are parked. Socket absence, `/health`, and another user's heartbeat do
not renew ownership. The box verifies the Access JWT using the configured team
issuer/JWKS and AUD, then matches the body `box` exactly to `TEALSTREET_ROUTER_BOX_ID`. Missing or invalid
router identity causes a clear startup error with enforcement enabled; shadow
mode continues and reports `routerBoxId: null`. `TEALSTREET_BOX` is only a
display/Sentry label and is not an ownership identifier. The router
must return public 404 for `/lease/*`, as for `/proof/*` and `/admin/*`.

`POST /lease/router-heartbeat` returns
`{accepted, untracked, checkedAt, enforcing, graceUntil, bootId}` with Unix milliseconds.
`enforcing` is true only with enforcement on and no active explicit enable grace.
The ACK stays conservative for the whole submitted batch, including users whose
first heartbeat has already ended the missing-heartbeat exception. Users share
one optional absolute grace deadline. `graceUntil` is that active deadline, or
null when enforcement is off or grace is absent/over. There is no implicit
startup grace. The configured deadline is not extended on restart; grace expires
when either its wall-clock or monotonic deadline passes, and never reopens in
that process after a clock rollback.

`bootId` is a random UUIDv4 generated once per process at startup, shared by
`/health` and every heartbeat ACK. Both responses are `Cache-Control: no-store`.
The router binds enforcing evidence to the exact grant and current boot ID;
observing a different boot invalidates older evidence. Reconnecting to an owner
must force a new heartbeat rather than coalesce with an old ACK. No-proof
takeover remains disabled by default and must not be enabled until both the box
and router changes are deployed and re-reviewed. An ACK is not a release proof
and never replaces the persisted maximum-issued `notAfter` and skew wait.

A batch contains 1–256 distinct users, user IDs at most 512 bytes,
and a body at most 256 KiB. Epoch and sequence are positive JS-safe integers.
Old or duplicate tuples never extend a grant; invalid assertions return 400.
A valid batch skips only assertions that cannot fit the inventory, returning
`untracked` beside `accepted` in the ACK; later existing users still renew.
An all-overflow batch returns 200 with `accepted:0` and a nonzero `untracked`.
The router's singleton renewal requires `accepted:1`, so overflow proves no grant.
A separate bounded peer limiter allows a 1024-request burst and 512 requests/s,
without consuming the proof endpoint's smaller allowance. There are at most
4096 tracked users; epoch high-water marks are retained until process exit and
capacity exhaustion refuses new private starts when enforced. Read-only socket
fence checks never register users: an unknown user follows explicit missing-user
grace below capacity, and is fenced whenever enforced at full capacity.
No raw user IDs are logged.

`POST /admin/router-lease/status {userId}` uses the existing on-box admin guard
and returns `userTag`, `enforced`, `wouldFence`, `fenced`, `ownerEpoch`,
`sequence`, `notAfter`, `generation`, `routerBoxId`, and `missingHeartbeatGraceMs`. It grants
no placement, renewal or write authority. Lookup does not register a user;
untracked users return 503 without changing inventory. Expiry uses both wall-clock
`notAfter` and a monotonic deadline so a clock rollback cannot extend ownership.

`POST /admin/router-lease/summary` accepts an empty body or `{}` under the
same on-box admin guard. It returns only
`{routerBoxId, enforced, enableGraceUntil, capacity, remaining, trackedUsers,
fencedUsers, wouldFenceUsers, liveWouldFenceUsers, liveUsersWithoutLease, checkedAt}`;
no user or account IDs.
`capacity` stays 4096; `remaining` is capacity minus the same snapshot's
`trackedUsers`. Inventory holds distinct users for the process lifetime, not
just active users: accepted epoch/sequence high-water marks are never evicted.
One count-only journal warning is emitted at each 80% and 95% crossing (3277 and
3892 users); plan a drain/roll before exhaustion. Safe eviction would require a
future authenticated router retirement watermark; expiry alone is insufficient.
`wouldFenceUsers` counts the exact per-user `wouldFence` predicate, including
wall-clock and monotonic expiry, independently of shadow/enforcement mode.
`fencedUsers` counts the per-user actual `fenced` flag. Hosted users with private
accounts (including parked accounts) or authenticated sockets are deduplicated;
those with no lease record count as `liveUsersWithoutLease`, and those whose record
has `wouldFence` count as `liveWouldFenceUsers`. `wouldFenceUsers` still counts every
tracked record, so `wouldFenceUsers - liveWouldFenceUsers` is departed users: a user
who moved home to the other box keeps an expired record here. Pending authenticated
upgrade reservations count as sockets. Anonymous public data and drain latches
alone do not count as hosted activity. Reading the summary never creates lease
records for absent users. `enableGraceUntil` is the active explicit enable-grace
deadline only while enforcement is on, otherwise null. It uses the same
absolute/monotonic expiry predicate as the heartbeat ACK and never grants grace
on restart. Without an explicit active deadline, ungranted tracked users count
as fenced immediately in enforcement mode. Operators gate enabling on both boxes reporting
`liveUsersWithoutLease == 0 && liveWouldFenceUsers == 0`; departed records alone do not
block it. This is a current-state
snapshot, not an admission latch or takeover proof.

### Rollout and recovery

Enforcement defaults OFF. In shadow mode heartbeats and would-fence state are
recorded, but IO and algos continue. Roll out the box image with enforcement off,
then router heartbeats and verify shadow state. For the first flip roll only,
the operator may set `TEALSTREET_ROUTER_LEASE_ENABLE_GRACE_UNTIL` to a fixed
absolute deadline (normally 90 s ahead), then enable enforcement. Remove the
grace env after this rollout; any retained past deadline grants no grace.
Environment changes take effect on restart. With enforcement on and no active
explicit deadline, missing/unknown users are fenced immediately, including after
a restart; only a valid authenticated heartbeat naming them can authorize IO.
An existing assertion always expires at its own `notAfter`, even during an
explicit enable grace. Ordinary rolls drain users first. The router sends fresh
per-user ownership within 10 s; socket reconnects do not reopen missing authority.

After enforcement, each account holds generation-bound authority. A fresh newer
assertion can authorize a rebuilt account, but cannot revive an old account's
queued work. Init captures this authority before reconnect pacing and carries it
through the engine lane and final IO guard. A newer same-name init with different
settings supersedes an unfinished build; publication checks both the captured
authority and init incarnation, so late results cannot replace the current account.
Missing/expired ownership rejects new account, order and algo starts
with the typed `ServerDraining` refusal (`venueUnavailable`, `notSent`). The final
native HTTP handoff and every WebSocket send also check authority, including
prepared HTTP, messages queued behind a socket handshake, and algo continuations.
All private-account outbound IO is stopped while that account retires; unrelated
users and public sources continue. No request is replayed on recovery.

`Registry::park_expired_router_leases` sends `AlgoInput::RouterFence`: it pauses
TWAP scheduling, stops chase repricing, aborts algo continuations and retires the
account host. It does **not** cancel resting/manual/chase orders. A request already
handed to the network may still complete or fill, and has no fabricated canceled
outcome. The same generation check on session cleanup prevents ordinary disposal
from racing expiry into cancel-on-dispose. Reconnect builds a fresh account;
restored running TWAP state is forced paused after a lease generation change.
The user checks Open Orders, explicitly resumes TWAPs, or starts a new chase.

### Interruption warnings

Box break-glass drain and forced user drain retain their existing #8699 forced
algo notice before closing sockets. Self-fencing also broadcasts a forced notice
and retains it for same-box reconnect: chases stopped, TWAPs paused, resting
orders kept, an in-flight order may still fill, and explicit checking/resume is
required. A disconnected user receives the retained notice on account init.

For cross-box recovery the router persists a one-shot interruption marker in the
user DO, strips client-supplied copies, and adds
`X-Tealstreet-Interrupted: {"reason":"owner-released"|"lease-expired","fromBox":"01","at":<ms>}`
on the next successful upgrade to a box. The box accepts it only alongside a
valid Access JWT and the normal authenticated user's proxy token. On successful
private account init it sends one #8699 forced notice for that user, saying the
previous server's algos stopped and must be checked. Missing, malformed or
unauthenticated markers do nothing. Repeated upgrades with the same marker do
not duplicate the notice; failed enqueue preserves it for the next account init.
`fromBox` is a bounded opaque router ID, with no validation against the display
name. When it matches this box's router ID, the notice uses the configured
`TEALSTREET_BOX` display name; otherwise it displays the router ID as-is.
A fresh destination at generation zero has no server-side jobs from the dead box
to restore. However, NX can send persisted client TWAP state at account start.
For either interruption reason, unacknowledged restored jobs are forced paused
even after the one-shot notice has been delivered. Progress and stable IDs are
preserved. A successful explicit `twap:resume` that actually makes the paused job
running acknowledges only that user/account/job and the observed marker plus
lease generation. Later account rebuilds preserve that job's client state;
another job or account stays protected. Invalid, refused or ineffective resume
attempts do not acknowledge. A newer marker or lease generation re-arms the job.
No-marker/no-fence restores keep their normal behavior. Because there is no
complete restored-job census, the marker is retained for at most 24 hours,
measured monotonically and bounded by its original timestamp; it is not cleared
when the first job resumes. Lease-generation restore guards also expire after
24 hours without relaxing IO ownership. Acknowledgements are bounded to 4096 per
user and 4096 users; a full ledger refuses resume before changing job state. There is no chase/EXEC restore operation in the current hosted API;
new restore operations must join this gate. The forced notice tells the user to
check their algos and orders before resuming. This does not invent a job handoff
or durable restore from a dead box; that remains psmp2r-39. Same-box lease-fenced
restore uses the same pause policy through its retired generation.
The bounded process-local marker ledger is not an external durable job store.

### Future: long-lived job handoff

Sam's intended future semantics are: "an external store holds the job queue, job
state transitions happen in memory, draining only has to wait for all jobs to
move to their next state transition, and new transitions are paused." Drain will
pause new transitions, wait only for in-flight transitions to commit (seconds,
not the job lifetime), then hand off; the next box picks jobs up from the store.
No external store or transition-engine refactor is introduced here.

Today's logical admission hook is `user_drain::User::admit`; the final physical
transition-start gate is `NetworkTransport::with_io_guard`, `http::Bound::dispatch`
and the final socket send (including the queued HOW job's final send). A future shared transition token would start there
and commit on `HostEvent::HttpEnd`/`HttpFailed` plus owned command completion in
`Port::settle`; ambiguous network outcomes must be reconciled before handoff.
A chase would split placement, fill reconciliation and cancel/reprice into
transitions. A TWAP would split due-slice admission, placement reconciliation and
schedule advance. The existing algo lifecycle is unchanged today.

`user_drain::User::snapshot` (`algosDone`, `pendingStarts`, readiness),
`Users::idle_release`, and `Registry::live_here`/parked ownership are the hook points for a future
jobs-movable / committed-handoff alternative to jobs-finished. The drain latch
must stay installed while that decision commits. Capture a user-owned,
serializable stable-job-ID record before `AlgoInput::RouterFence` disposes the
scope: `TwapBook::serialize_state` carries options, progress and in-flight state;
`ActiveChaser.id`, `ChaserBook::progress` and `currentOrderIds` identify chase
resume/reconciliation state. Ownership and these records must be independent of
the socket and box. Today's paused TWAP restore and interruption warning require
explicit user action; no speculative restore or automatic cross-box replay runs.

## Authentication

- **Token check:** `proxyToken` is an RS256 JWT, verified with today's `PROXY_PUBLIC_KEY`. That key is copied verbatim into `src/auth.rs`.
- **Claims:** it needs a nonempty string `userId` claim and an unexpired `exp`.
  - Once verified, the `userId` binds both the socket and any resume.
  - Optional `chatHandle` is the authenticated user's `chat_users.username`,
    supplied by NX when minting the signed proxy JWT. Only 1–32 ASCII letters,
    digits, `_` and `-` are retained; absent, non-string or invalid values fall
    back to the tag alone without refusing the token. No email/name fallback.
  - Session-start and existing engine account labels append `chat=<handle>` to
    the existing 12-hex user tag. The handle grants no access and never replaces
    `userId`, `user_tag`, account keys, routing or resume ownership. Browser init
    fields and query parameters cannot set it. Existing account labels retain
    their build-time value; the session-start line maps each fresh token's handle.
  - At `exp + 30s` the socket closes with 1008. That 30s is the same reconnect grace as `PROXY_TOKEN_RECONNECT_GRACE_MS`.
- **Use of the token:** it is also the per-socket proxy token behind the account's `ProxyTokenProvider` (see Routing). It is never logged.
- **Private COSIGNED join proof:** fresh connections use the same isolated native
  probe and admission path as DELEGATED, independently of `delegated.enabled`.
  Exact `bybit`, `binance`, `okx`, `bitget`, `blofin` and `kucoin` are proof-ready.
  Other private COSIGNED venues return an init refusal with
  `kind: "proof-unavailable"`, `initRefused: true`, `transient: false` and
  `outcome: "notSent"`; they do not fall back to registry attachment.
  The pending connection signs only its own server-built requests, receives no
  resident account state, and cannot issue methods or start jobs. Only a proved
  connection becomes a holder/signer. Existing same-connection snapshots and
  same-user COSIGNED resume retain their behavior. See
  [the host contract](../../../docs/plans/callers/cosigned-join-proof.md).
- **Unauthenticated sockets:** they may only `init` the public pseudo-account `__pub_bybit`.
  - The validation is the same as `validateUnauthenticatedPublicInit`.
  - Only `sendSnapshot` and the subscribe/unsubscribe book and OHLCV `msg` ops are allowed.
  - They can never send `sign-res` or `verbose`, and either one closes the socket with 1008.

## Envelope

All application messages are JSON text frames of the form `{"t": <type>, "d": <data>}`.

### Client to server

| `t` | `d` | Behaviour |
|---|---|---|
| `init` | `InitPayload` (same fields as `server.ts`) | Builds a new account or re-snapshots matching settings; changing an existing private account requires `apply-settings` |
| `apply-settings` | `{accountName, instanceId}` | An admitted private client explicitly stops its current account before reinitializing with new settings |
| `msg` | `{op, data?, accountName, instanceId?}` | Account op, see below |
| `sign-res` | `{id, config?, wsAuthSignature?, error?}`, with `ts` as a sidecar | Goes to the signer (4yjlw0) |
| `echo-res` | `{id}` | Echo RTT sample |
| `ping` | `{accounts?: string[], link?}` | Replies with `pong`. Then reconciles: when `accounts` is nonempty, any server account not in that list is disposed |
| `dispose` | `{accountName}` | Disposes the account |
| `perf` | `{level}` | Session performance level. The server maps it to the engine's `setPollScale` |
| `verbose` | `{enabled}` | Authenticated sockets only. Diagnostics |

`msg` ops:

| `op` | Behaviour |
|---|---|
| `callMethod` | `data: {requestId, method, args}`. Becomes an engine `Command` (see Trading); reply `callMethod:done` |
| `subscribeOrderBook`, `unsubscribeOrderBook` | `data: {symbol}`. Becomes the engine book subscription |
| `subscribeOHLCV`, `unsubscribeOHLCV` | `data: {symbol, interval}`. Becomes the engine candle subscription |
| `sendSnapshot` | Sends a full `snapshot: true` frame to this socket |
| `updatePositions` | Answered `updatePositions:done`. Positions are already engine-owned |
| `exec`, `chaser:*`, `twap:*` | v1 replies `error` "not supported on the Rust exchange server". These are TS worker features; the post-cutover work is listed below |

### Server to client

| `t` | `d` | When |
|---|---|---|
| `session` | `{resumeId, resumed}` | On open, for `resumable=1` sockets |
| `msg` | `{op, accountName, data, ...}` | Account output (see Frames and Replies) |
| `sign-req` | the signer request (4yjlw0) | Remote signing |
| `echo` | `{id, pad}` | Echo probe every `SIGNER_ECHO_PROBE_MS`, default unchanged |
| `pong` | `{accounts: string[], ...linkReport}` | Reply to `ping` |

`apply-settings` reuses the settings-disposal path: algos stop with the recorded
reason "its key or connection settings changed" (`settings_changed`). It accepts
only the current live account/registration with unexpired socket authority.
The nonempty `instanceId` must match the admitted init. A legacy client can first
repeat its unchanged init with an ID to bind the same account without restarting it.
Pending verification replies `verifying`; unadmitted, stale or fenced clients
cannot reset an account. Live and parked holders, and a DELEGATED resident copy
without a socket, prevent an ordinary init from changing private settings.
The comparison uses normalized engine and routing settings; fields ignored by
the venue do not create a settings change.

The action replies `msg` with `op:"applySettings:done"` and
`data:{success:true,requiresInit:true}` on the admitted registration. This means
the reset was initiated, not that a replacement was admitted or all venue work
settled. The caller then sends a normal init (including fresh DELEGATED handover
and verification when applicable). The action carries no credentials or new
settings, and older pending joins cannot restore the reset account. Existing
init generations, account outputs and settings stop notices retain ownership.

Socket keepalive: the server sends a protocol-level WebSocket ping every 10s.
With no inbound data for 30s, either a message or a pong, the socket is
dropped. This matches Bun's `idleTimeout: 30` with `sendPingsAutomatically`.

### DELEGATED handover

DELEGATED uses the same hosted eligibility as COSIGNED: a supported parsed
`remoteSigning` policy and a configured server origin, including the existing
unhosted refusal. There is no per-venue DELEGATED list. Authenticated connections
request a handover with `msg` data
`{op:"delegated-key-req", accountName, data:{exchangeName}}`.
An unjoined caller must supply `data.exchangeName`; a joined account may omit it
and use its existing venue. Missing or unsupported targets refuse without an
offer. The `msg` response has
`{op:"delegated-key", accountName, attemptId, publicKey, bootId, expiresAtMs}`.
The public key uses padded standard base64. An offer lasts 30 seconds, belongs
to that connection and saved account name, and is replaced by another request
for the same name. Each connection retains at most 64 unexpired offers.

The matching init adds `signingMode:"delegated"` and
`sealed:{attemptId, enc, ciphertext}`. There is no Bybit-named init field.
DELEGATED uses the shared hosted eligibility for the requested venue.
Fresh private COSIGNED inits require possession proof independently of
DELEGATED hosted eligibility, including clients omitting the mode field. Only
the exact COSIGNED proof-ready venue names can complete COSIGNED admission;
other private COSIGNED names refuse without attaching an account. Matching inits on
an already-admitted connection, including a resumed account, retain their
snapshot-only behavior. Account settings and engine options remain unchanged.
Plaintext credentials are never init settings or account fingerprints.

The handover uses RFC 9180 base mode with X25519/HKDF-SHA256,
HKDF-SHA256 and ChaCha20-Poly1305. The application info is
`tealstreet/delegated-handover/v1`; associated data joins the attempt ID, process
boot ID, exchange name and init key with `|`. `enc` and `ciphertext` are padded
standard base64. Decrypted JSON contains `key`, `secret` and optional
`passphrase` and optional `signingKey`; its public key must match the nonsecret
init key. The registry's key-only setup takes its private scalar exclusively
from sealed `signingKey`, with the public init identity equal to `subkeyAddress`.
Other credential mappings refuse that field and still require their signing
secret. The previous scalar-in-init layout refuses without a fallback. The
canonical trusted setup validates credential material before the same probe.
The opener and parsed credential
fields erase their owned buffers on drop. Handover parsing retains the JSON
string contract, including escaped strings and optional or null `passphrase`.
JSON input is limited to 12,272 bytes, consistent with the envelope limit.
Opening fills the shared `delegated_memory::CandidateCopy` directly after the
requested venue's preparation succeeds. The offer retains its target venue,
which must still be enabled and match the init when opened. Handover replies use the current init registration,
including its optional `instanceId`; clients without an ID retain the same bytes.

Opening and subsequent join verification run in a spawned task. The existing
socket select loop handles its completion, while continuing ping, close and
drain handling. Until verification completes, commands for that account receive
`verifying`, and the account is not joined or subscribed. Completion belongs to
the captured user, socket and init generation; dispose, another init, another
key request or socket close retires pending work. Verification failure leaves
any running account unchanged.

Both modes use a temporary native account for the requested venue's normal private
startup reads, disposed before admission. COSIGNED uses a pending signing link
owned only by the joining connection; DELEGATED uses the supplied pair through
the existing trusted `prepare_local_signer_for` mapping. Wallet public addresses
remain public engine metadata; key-only private material is consumed by its
existing setup rather than treated as an HMAC pair. Setup's opaque
`TrustedSignerState` accompanies the provider through native `ServerConfig` for
both probe and resident construction. It preserves non-deserializable facts
such as Paradex Ethereum-derived-key validation without exposing material or
letting JSON assert those facts. Reconfiguration retains the resident's public
signer metadata and opaque facts. The shared completion rechecks the token and captured
router lease before attaching the account. Pending signing links are retired on
completion or cancellation.

The shared admission timeout is the existing remote signer's `SIGN_TIMEOUT`
plus the native private-read `RequestClass::Read` budget (currently 15 s + 20 s).
The same timeout covers pacing and the complete probe for both modes, without
adding a timer or resetting the budget between stages. Expiry retains the
transient refusal and temporary-account/pending-signer cleanup.

An accepted pair starts a DELEGATED account when none exists, or joins the
matching running DELEGATED account while retaining its existing signer. This
does not compare secret material or replace an existing account's settings.
The probe's router-lease generation remains attached through native construction
and outbound IO. Its account is also disposed when the pending init is canceled.

Verified admission binds the account's output to its socket-local generation
before construction can defer replies. Joining a different native account retires
the previous generation and refreshes the holder's outbox; joining the same
account keeps its generation. Pending handover replies retain their separate
registration lifecycle.

Probe refusals use `initRefused`, `kind:"unverified"` and the existing fixed
message. Auth kinds are permanent refusals; rate, network, timeout and venue
availability failures are transient. Replies retain the init's registration ID;
clients without one retain the original envelope bytes. Verified admission does
not replace a running account with different settings. A supported private
COSIGNED venue without proof readiness returns the permanent `proof-unavailable` init
refusal before probe I/O or attachment; unsupported names retain `unsupported`.
DELEGATED retains its shared hosted eligibility and private-startup proof.
Public-only accounts, unsupported/unhosted venues and existing early-subscription
handling retain their previous paths. Fixed synthetic interoperability and RFC
test data live in `tests/data/`.

The shared startup admission runner makes exact `bybit`, `binance`, `okx`,
`bitget`, `blofin` and `kucoin` proof-ready for COSIGNED. It runs the selected
venue's existing native private startup with either the remote provider or the
local provider; it does not build a second set of REST requests. Its result
classifier uses engine `ErrorKind` values: authentication failures are invalid,
rate limits and transport failures are transient, and complete private startup
is accepted.
Unclassified startup failures remain transient refusals. Public-only and
frame-fed accounts cannot satisfy this check.

COSIGNED proof readiness is independent of DELEGATED hosted eligibility.
The COSIGNED catalog does not gate DELEGATED admission. COSIGNED coverage
includes valid startup, a different signing key, a prior signature and a
consumed response. The DELEGATED offline admission
suite additionally covers a different handed-over pair and a reused sealed
handover. The suite verifies the actual native HTTP signatures against synthetic
venue responses. Existing Bybit session tests additionally cover resident-account
and registration preservation. These offline checks do not replace live venue
validation or the existing server-origin and signing requirements for rollout.

## Frames (claude-19pt8s, web hosted client)

The browser host (`ExchangeServerWorker.ts`) forwards only TEXT frames to the
client, as `{frame: '<raw string>'}`, and a React Native WebView bridge carries
strings only. So SCXF rides inside the existing JSON envelope, base64-encoded.
Binary WebSocket frames are out of scope for the cutover.

Server to client, one message per engine flush per account:

    {"t":"msg","d":{"op":"frame","accountName":"<name>","seq":<u64>,"snapshot":<bool>,"data":"<base64 SCXF>"}}

- **`seq`:** monotonic per account per server session. It starts at 1 on init and on resume, and again on every full snapshot.
- **`snapshot: true`:** marks a full native snapshot. It replaces the client's catalog, book and order baselines.
  - The server sends one immediately after a successful init, and after every `sendSnapshot`.
  - A socket that joins an account later gets its own snapshot before any delta, which is FrameServer's join contract.
- **Client side:** the client keeps one `FrameApplier` plus `DefaultStore` per account, and applies frames in `seq` order.
  - A `seq` gap, an `Invalid engine frame` or a `FrameApplyError` makes the client send the existing `{t:'msg', d:{op:'sendSnapshot', accountName}}`.
  - It then ignores deltas until a `snapshot: true` frame arrives. This reuses today's `hostedFrameClient.requestSnapshot` rate limit.
- **Size:** FrameServer already splits a delta frame that is too big under `max_frame_bytes`. Each segment is its own `seq`.

State that SCXF does not carry stays JSON, under the existing op names and shapes:

- **`op:'store'` partial payloads:** these carry only the fields frames don't (`fetchedLeverage`, `balance.info`, `socketHealth`, `messageHandlerStats`, `activeChasers`, `activeTWAPs`, `ordersRisk`). They are merged over the frame store, which matches the worker's `_derivedStorePayload` in frame mode.
- **Unchanged replies:** `error`, `warning`, `log`, `engine-event`, `command-progress`, `callMethod:done`, `exec-result`, `updatePositions:done`, `chaser:*` and `twap:*`.

Account output captures a socket-local account generation before a callback,
detached read, signer or frame pump can defer it. An accepted rebuilding init
retires that generation before obtaining the replacement; dispose and orphan
release retire it too. Both enqueue and socket dequeue check the captured
generation and discard stale output, including delayed algo stop notices and
already-queued frames and account-scoped init refusals, including signing-mode
mismatches. Repeated init and re-snapshot of the same account keep their
generation. Other accounts and session envelopes are unaffected.

An optional string `init.instanceId` identifies the browser registration, not
the account or its authority. It never enters engine options, fingerprints or
rebuild comparisons. Account `msg` replies and remote `sign-req` frames echo it
as `d.instanceId`, matching NX #9342's TypeScript reply envelope. Absent or empty
IDs leave the original serialized bytes untouched.

Rebuild refusals notify once per account, reason and registration ID, including
when retries for registrations interleave. Without an ID, the legacy cache still
notifies only when the last reason changes. Cache lifetime and clearing remain
unchanged; this cache never changes fingerprints or rebuild admission.

Each joined connection owns its reply output, so devices sharing one account
receive their own IDs. Same-engine re-init and resume replace that connection's
output before the new snapshot; queued frames and detached replies retain their
captured output. Account-holder broadcasts stamp separately for each recipient.
Each init refreshes fallback reply provenance after the connection's policy
checks, even when rebuilding is refused or the venue is unsupported. Native
disposal or join cleanup cannot restore an older registration. Explicit client
disposal removes it. Typed order receipts select the admitted joined registration's
output, rather than a later refused init's fallback. Pending verification attempts
retain their own refusal output; stopped and unjoined accounts retain the latest
fallback provenance. This metadata grants no engine or signing authority; outputs
already captured by old clients, commands or callbacks keep their original ID
while their account generation is current; retired generations are dropped.
Retained records capture their source account fingerprint before deferral and
are stored separately by user, name and fingerprint. Replay selects the actually
joined fingerprint before stamping the new holder's ID, so retired credentials
cannot donate a notice or overwrite another account's owed record through an
empty registry slot. Same-fingerprint rebuilds preserve resume-owner, once-only
and 24-hour retention rules. Session envelopes and all existing account/lease
authority checks remain unchanged.

A resumed signer can reattach before the browser re-inits. Its unsent signing
requests wait in a queue bounded by the socket's outbound capacity until the
same account's registration is known. An accepted replacement discards those
old signing requests instead of attributing them to the replacement. Ordinary
legacy account messages before re-init retain their existing delivery.

How FrameServer notices map onto `msg`:

| FrameServer notice | `msg` op |
|---|---|
| `done` (result or error) | `callMethod:done` `{requestId, result}` or `{requestId, error}`. `error` is `SerializedCommandError`, with `outcome:'unknown'` for writes whose result is unknown |
| `event` (level, code, message, fields) | `engine-event` |
| `deactivate` | `deactivate-status` |
| `startupFailed` | `start-status` `{ok:false, detail}`, plus `error` |
| `accountAuth` | `engine-event` with code `accountAuth`. The server always opts in |

### Recovering a refused native startup

FrameServer retains `StartupFailed` for late joins. Only a failure originating
in `Cx::startup_read_refused` while required private startup was incomplete
can retire: that same source engine must load markets, balance, positions and
orders with no signing refusal. Public-only engines, frame-fed mirrors, partial
state, terminal engines and non-read failures (including a missing signing key)
cannot clear it. A new failure on an already-loaded account is retained.
This proves bootstrap completion, not that an agent key can place an order.

Connected peers receive the recovery store frame. NX re-derives essential
readiness and clears health/terminal failure in
`exchangeAccountController.ts:2574-2624`; no new wire notice is needed. That NX
branch currently checks loaded flags alone, so unsigned frames can also clear
its UI latch for a non-read signing failure. This pre-existing NX limitation is
separate; native late-join replay retains that failure.

An auth-kind startup refusal may receive at most three native read-only
rechecks, after 5, 10 and 20 minutes respectively, each with 0–30 seconds of
host entropy jitter. A longer venue `retryAfterMs` floor is preserved; a new
refusal does not reset the attempt budget. Clock rollback cannot accelerate
the monotonic schedule. Successful completion retires the remaining rechecks.

The venue must opt in via `Venue::rechecks_startup_reads` and implement
`Venue::recheck_startup_reads`; non-adopters return before any engine turn.
Hyperliquid is the first adopter:
it retries only identity/mode and account State/Spot/Orders reads through the
existing single-flight and admission paths. These hooks are absent from wasm.
The host never restarts the venue, recreates the actor, replays a command or
fires arbitrary venue timers. Existing algo/job state stays on that actor.
After exhaustion, the failure remains visible and an explicit account retry
can start over; the page's public-only refusal latch is a separate policy.

The client's output to its own consumer stays as today: `op:'store'`
snapshots, `listenOrderBook` (from `FrameApplier.books`), `listenOHLCV` (from
`.candles`) and `fill` (from `.fills`). Nothing downstream of
`hostedFrameClient` changes.

Decode uses `@tealstreet/safe-cex-wasm/wire` `Frame`, which is pure JS and
needs no wasm on the client, plus safe-cex `FrameApplier`.

No version gate (hard cut): the client handles `op:'frame'` unconditionally and still accepts `store` and `store:delta`.

## Trading (callMethod to engine Command)

`callMethod` keeps its op names, `requestId` and `callMethod:done` lifecycle.
The server translates `method` plus `args` into the same engine command JSON
that `RustEngineExchange` sends to `WasmEngine.command`, and decodes it with the
shared typed decoder (`HostInput`). Module: `hosts/server/src/methods.rs`
(fe7lgm).

- **Mirror of the TS translation:** it mirrors `rust.exchange.ts` method by method. Golden tests record TS `method(args)` to engine-command pairs from the TS translation, and the Rust module must reproduce each pair.
- **Cutover set:** `placeOrder(s)`, `placeOrderWithClientOrderId`, `updateOrder`, `cancelOrders`, `cancelSymbolOrders`, `cancelAllOrders`, `findOrderByClientOrderId`, `setLeverage`, `fetchLeverage`, `setMarginType`, `setPositionMode`, `fetchPositionMode`, `nuke*`, `fetchOHLCV`, `fetchPublicTrades`, the `fetchHistorical*` reads and `fetchIncomeHistory*`.
- **Unknown methods:** they fail with `callMethod:done` `{error: {name:'UnsupportedMethod'}}`. They never get a guessed command.
- **No retries:** the server never retries a trading command. A disconnect never cancels or replays a write, which is FrameServer's orphan rule.

## Accounts and runtime (fe7lgm)

Engines are `!Send`, so accounts live on account threads: `TEALSTREET_ENGINE_THREADS`
OS threads, each running a current-thread tokio runtime plus a `LocalSet`. A new
account goes to the thread with the fewest accounts. The accept loop, HTTP and
the per-socket envelope tasks run on a multi-threaded tokio runtime. They reach
accounts only through `Send` channels.

The FrameServer library change, in `hosts/native/src/server.rs`, keeps the
existing `serve()` behaviour and wire byte for byte:

- **`AccountRegistry`:** registers and removes accounts at runtime, while serving. Today the account map is fixed before `serve`.
- **`AccountHandle` (`Clone + Send`):**
  - `join(client, account_auth) -> ClientLink`. `ClientLink` yields `Outgoing::{Frame, Text}`; the first item is a snapshot frame.
  - `command(client, Command)`, `poll_scale(...)`, `leave(client)`.
  - `dispose()`, which stops the actor, closes its venue sockets and settles pending jobs as unknown.
- **`connection()`:** the existing WebSocket wire becomes one consumer of `AccountHandle`. `hosts/server` is a second consumer.

Sizing, for 110 to 1,200 accounts per region (from the CI replay data):

- **Runtime:** a multi-threaded tokio runtime for sockets, plus `TEALSTREET_ENGINE_THREADS` account threads. FrameServer's own example is single-threaded and caps the whole server at `max_connections` 256; neither limit applies here.
  - Each account still takes `ServerConfig` per-account limits.
  - The server caps sockets at `TEALSTREET_MAX_CONNECTIONS` (default 4096).
- **Reconnect pacing:** account starts are paced at 5 to 10 per second per egress IP, with jitter. A mass reconnect queues inits rather than stampeding venue REST and WebSocket logins. Queued inits still get their snapshot when they start.
- **Shared public feeds and market loads (follow-up after the first slice):**
  - one public engine per venue and region;
  - account engines use `PublicFeedPolicy::Shared`, fed through `HostEvent::PublicFeedUpdate`;
  - markets are loaded once per venue.
  - This halves the venue socket fleet and protects public rate limits on mass reconnect.

Account identity:

- **Hosted Bybit private accounts:** `(verified uid, identity digest)`, for both hosted signing modes. Each connection maps its account labels to that identity and receives its own label and current `instanceId` on account frames. Different labels for the same identity share one actor. This existing identity namespace is independent of venue-generic admission; other hosted venues keep their current account-label identity.
- **Other accounts:** `(userId, accountName)` retains its existing behavior.
- **Different users:** never share an account, even with equal names.
- **Unauthenticated public:** `__pub_bybit` is a single shared, public-only bybit account.

Label lookups use the connection's registered labels. A conflicting legacy
label binding is refused without rebuilding either account. Algo broadcasts
reach every registered label while counting each socket once.

The identity digest is lowercase SHA-256 hex. Its input starts with the UTF-8
bytes `tealstreet/identity/v1`, followed by three UTF-8 components: verified uid,
scope, and canonical public key id. Each component is prefixed by its byte length
as an unsigned 64-bit big-endian integer. Scope is the compact JSON array
`[venue, network, site]`, where network is `mainnet` or `testnet` and an absent
site is the empty string. Credential trimming follows the venue's signer setup
policy. The digest uses no process salt. Account labels and signing secrets are
excluded. Enabled Bybit settings fingerprints exclude primary credentials.

Settings changes on a held Bybit identity use the admitted connection's
`msg` operation `applySettings`, with `data: {requestId, settings: InitPayload}`.
The settings must name the same label and identity. Replacement construction and
its captured authority checks succeed before retiring the resident account through
the existing settings-disposal path. A failed replacement keeps that resident;
DELEGATED retains the same provider. The captured token deadline is rechecked
before engine placement and publication; expiry keeps the resident intact.
The reply is `applySettings:done` with
`{requestId, success, result}` and the current registration's `instanceId`.
Parked holders and the DELEGATED resident copy participate in settings refusal.

Bybit remote sign requests include `keyId` alongside the connection-local label.
Hosted Bybit TWAP persistence carries the identity digest in `ownerId`.
Restoration requires that identity and can follow a renamed label. Other TWAP
books retain their existing persistence shape. Connection-local labels and
registration IDs remain the account binding at the hosted write-frame boundary.

Account construction, from `init`:

1. Validate `InitPayload`. Apply the same normalization as `buildHostedAccountConfig`: the key is passed through, and there is never an API secret.
2. Build options JSON for `safe_cex_venues::build_with_signer`:
   - credential refs only;
   - the remote signer's `options_json` (`remoteSigning: true`, `payloadSigner: false`);
   - `hyperliquidBuilder` validated as in `server.ts`.
3. Get the routing plan from m26ps4's `resolve_routing_plan`. Build `NetworkTransport::with_routing(plan, token_provider)`. `token_provider` returns the socket's current `proxyToken`; after a resume it returns the new socket's token.
4. Register the account on an account thread, join this socket, and send a snapshot frame.

A construction error replies `msg` `{op:'error', accountName, data:[message]}`.
This is the same as `sendInitError`.

Unsupported venues, including saved BitMEX accounts, are refused before routing
or account replacement. Their error has `data:[message, {forceToast:true,
initRefused:true, kind:'unsupported', outcome:'notSent'}]` so the client stops
waiting for that account without disrupting other accounts on the socket.

Sessions and resume (same semantics as today):

- **Session id:** every socket gets a `resumeId`.
- **On close, `resumable=1` sockets:**
  - private accounts are parked under `resumeId` for 60s, bound to `userId`;
  - other accounts are disposed;
  - chases and TWAPs live with the account and keep running through the park (Sam, the COSIGNED blip window); a socket close no longer ends chases. A joining socket gets the algo store rows again.
- **Resume:** a new socket presenting `resume=<id>` with the same `userId` adopts parked COSIGNED accounts. It receives `session {resumed:true}` when at least one account resumes. DELEGATED accounts use a fresh handover and shared admission; their previous parked holders retain the original expiry while verification is pending.
  - Its first `init` with an identical fingerprint keeps the account and sends a snapshot.
  - A different private fingerprint is refused until an admitted client uses an
    explicit settings action; parked clients and resident copies keep their settings.
    The top-level `apply-settings` resets before re-init; identity-bound Bybit
    accounts also support the staged `msg applySettings` action described above.
- **Resume alias:** a used `resumeId` stays valid as an alias for 120s, which is `RESUME_ALIAS_MS`.
- **Grace expiry:** parked accounts are disposed with the reason `no browser for 60s`, recorded in the order audit log. No browser is attached to tell, so when chases or TWAPs were running the registry keeps one notice per (user, account) and sends it as an `error` on that account's next init; it is dropped once delivered or after 24 hours.
- **Stop notices name their algo.** Every "Chase on X stopped: …" or "TWAP on X stopped: …" (live, break-glass, SIGTERM, grace expiry, failed signer) is its own `error`, `data: [message, {forceToast: true, algo: {kind: "chase"|"twap", id}}]`, where `id` is the algo's `activeChasers`/`activeTWAPs` row id, so the page dedupes exactly. Errors the page missed still arrive joined in one notice without `algo`.

## Remote signer (claude-4yjlw0)

This has landed as the crate `safe-cex-remote-signer` (`sdk-rs/hosts/remote-signer`, #2381). `src/signer.rs` only wires it in.

- **`SignerLink::new(socket_id, outbox)`:** create one per account. `socket_id` is a `u64` the server assigns per client socket. It is the twin of TS `WebSocketRemoteSigner`, which is also per account.
  - `Outbox` is `Arc<dyn Fn(String) -> bool + Send + Sync>`. It queues one text frame on the account's owning socket and returns false when the frame was not sent.
  - The frames are `{"t":"sign-req","d":RemoteSignerRequest}`, with `t` first, and the timeout is the 15s `SIGN_TIMEOUT`.
- **`remote_account(link, venue, Some(accountName), engine_options_json)`:** returns a `RemoteAccount` with:
  - `provider`, which you pass to `build_with_signer` / `FrameServer::add_venue_with_signer`;
  - `options_json`, which you construct the engine with;
  - `on_fatal(cb)`, which fires once after 10 consecutive failures spanning at least 30s. A timeout is not a failure here: only an error or undecodable answer, or a socket that cannot take the request. It disposes the account, and the page re-inits when its pong no longer lists it.
- **Input:** `engine_options_json` holds the options the server would otherwise build. That includes the public `key` and an optional `passphrase`, with no secret.
- **Output:** `options_json` has those fields stripped and carries `signing` (opaque refs), `remoteSigning: true`, `payloadSigner: false` and `publicMode`.
- **`sign-res`:** call `link.resolve(socket_id, d)` only on links of accounts this socket joined. A request resolves only from the socket it was sent on, and only once. An oversized field settles as an error.
- **Resume:** on park, call `link.suspend()`. A sign already sent keeps waiting, and a sign requested while suspended (an algo order during the gap) waits for `attach`. Both wait within their one 15s `SIGN_TIMEOUT`, then fail as unavailable without counting toward the fatal streak. Neither is queued past the timeout, and the chase or TWAP reports the failed placement. On resume, call `link.attach(new_socket_id, new_outbox)`, which re-sends every pending sign over the new socket; only that socket's answer resolves it, and a sign it cannot take aborts. On dispose, call `link.mark_unavailable()`, which aborts everything. A resume naming a session that is still open for the same user (its socket dropped with no close frame, so the 30s idle check has not fired) takes it over: the old socket loop is told to close now, parks its accounts, and the new socket resumes them (`Registry::take_over`), so an abrupt blip is a resume, never a re-init. A sign that times out with nothing heard from its socket since it was sent is `Suspended` (no browser), not a signer failure, so the blip never counts toward the fatal streak.
- **Echo:** `link.send_echo()` alternates bare and padded-to-last-sign-req probes, one outstanding at a time. Feed `echo-res` `d` to `link.echo_res(d)`, which returns `(rtt, EchoSize)`.
- **Wire coverage:** the wire is the existing browser responder's ws-auth wire. That covers:
  - the HMAC families, with hex, base64 and hex-base64 encodings, a passphrase Present, and the extraData codecs `secondsTimestamp`, `bybitExpiry` and `binanceSpot`;
  - `bitunix-double-sha256`;
  - `hyperliquid-address` and `hyperliquid-l1`;
  - `lighter-tx`;
  - `paradex-public` and `paradex-stark`.
- **Address and key binding:** Hyperliquid's address and Paradex's Stark key bind through the engine's owned address setup (`create_public_address_setup`). This needs the native host's `address-nonce` feature for those venues.

## Routing (claude-m26ps4)

The module is `safe_cex_native::routing::resolve`. It sits beside the
`RoutingPlan` type that `NetworkTransport::with_routing` already applies, so the
server gets no new crate dependency.

Interface:

- **`account_transport(exchange, AccountTransport{tealstreetProxy, tealstreetTunnel, proxyRegion}) -> ResolvedTransport`.**
  - It ports `resolveAccountTransport`.
  - It also ports the metadata defaults that `accountToExchangeOptions` applies to a hosted account: `requiresCors`, `enforceWsTunnel`, `supportsTunnel` and `privateProxyRegions`.
- **`resolve_routing_plan(exchange, &RoutingInputs, real_rest_base_url) -> Result<RoutingPlan, RoutingError>`.**
  - `RoutingInputs` is the routing-relevant subset of the normalized `ExchangeOptions`: `tealstreetProxy`, `tealstreetTunnel`, `proxyRegion`, `bybitSiteId`, `tealstreetWsRoute`, `publicDataOnly`, `hasKey`, `payloadSigner`, `overrideProxyUrl`, the presence of a secondary key, secret and passphrase, `secondaryProxyUrl`, and the presence of a passphrase.
  - It carries no credential values and no JWT.
- **Relay:** off, as on today's hosted server, which never calls `setWsRelayEnabled`.

It reproduces the TS resolver exactly for the hosted (Tealstreet transport) case,
using the Vultr hosts from after NX #8306. Hosted shares the browser table:
tunnel, relay, proxy Worker and public CORS.

- **Source of truth:** one generated JSON file holds the per-venue routing data: the routing table, metadata transport flags, the `TOKYO_EGRESS` list and the hosts. A vitest test fails if the JSON drifts from the TS.
- **Goldens:** a vitest file records the TS `resolveEngineRoutingPlan` output, and a cargo test asserts the Rust output equals it.
  - Venues: every core venue (binance x4, bybit x4 plus hyrotrader, okx x4, bitget x3, Hyperliquid plus spot, blofin, bingx, lighter plus robinhood, paradex, phemex).
  - Matrix: proxy, tunnel, region, site, publicOnly and secondary.

The server calls it once per `init`. `CONNECT_REGION` supplies the region
default exactly where the TS server reads it.

## Crate layout (`hosts/server`)

    src/main.rs       env config, runtimes, signal handling
    src/config.rs     environment surface (table above)
    src/http.rs       axum router: /health, /capabilities, upgrade
    src/auth.rs       RS256 proxy JWT verification
    src/envelope.rs   serde types for every message above, golden-tested against server.ts shapes
    src/session.rs    per-socket task: envelope, keepalive, idle timeout, echo, reconcile
    src/accounts.rs   account threads, (userId, accountName) registry, park and resume
    src/methods.rs    callMethod to engine Command
    src/frames.rs     FrameServer Outgoing to msg frame/notice mapping, base64
    src/signer.rs     4yjlw0 (wires safe-cex-remote-signer)

Dependencies: `axum` (with `ws`), `tokio`, `jsonwebtoken`, `base64`, `serde_json`,
`safe-cex-native` and `sentry`. `sentry` runs without default features: only
its panic event builder and the reqwest transport, over the rustls/ring stack the
server already links. There is no native-tls, no host contexts and no sessions.

## Algos (claude-tbz5hr chaser, 55ao7t TWAP, zs5e83 exec)

Algos run server-side on the account's thread. They sit on the same `AccountHandle`
a socket uses, as one more in-process client:

- **Store:** `join()` yields the snapshot and then every flush frame. An algo keeps its own `FrameApplier`-equivalent Rust store, decoding with `safe_cex_engine` frame reading. The store is never shared mutably with the engine.
- **Commands:** `command(client, Command)` gives a completion through the same done notice. The no-retry and orphan rules apply.
- **Timers:** a tokio `LocalSet` on the account thread, so algos may hold `!Send` state beside the engine.
- **Lifecycle:**
  - Algos are owned by the account entry in `accounts.rs` and die on dispose. Chases and TWAPs keep running while the account is parked.
- **Wire:** algo ops (`chaser:*`, `twap:*`, `exec`) reach the algo through a registry in `accounts.rs`: `fn algo(op) -> Option<&dyn AlgoFactory>`. Replies use the existing op names.

## Multi-node (fe7lgm; post-cutover design, not implemented)

Today `connect-tyo` is one Cloudflare tunnel to exchange-01. exchange-02 is a
cold standby with cloudflared off. Losing exchange-01 drops every hosted user
until someone starts the standby by hand. This section plans two boxes
without letting one account run on both.

### Invariants

1. **One owner.** A private `(userId, accountName)` has a running engine and algo host on at most one node at a time. Two owners could mean two TWAP schedules or two chase loops placing orders for the same key.
2. **Resume where issued.** A `resumeId` only means something on the node that issued it, because parked accounts live in that process's memory.
3. **Fail closed.** If a node cannot tell whether it owns an account, it does not run algos for it. A user can always recover with a fresh `init`.

### Topology: active/standby behind a load balancer

- **Both boxes run the server.** Each has its own Cloudflare tunnel and cloudflared. Neither box is cold any more.
- **Per-node hostnames.** Each tunnel serves `connect-tyo-<node>.tealstreet.io`, for example `connect-tyo-01`. It is used only for resumes.
- **`connect-tyo.tealstreet.io` sits on a Cloudflare Load Balancer:**
  - the primary pool is exchange-01's tunnel and the fallback pool is exchange-02's;
  - pools are health-checked on `GET /health`;
  - it is failover, not round-robin, so all fresh inits land on one node and invariant 1 holds without coordination in the steady state.
- **No automatic fail-back.** After a failover, the operator marks the old primary's pool disabled until a quiet window. Otherwise new inits split across both nodes while old sessions still run on the other one.
- **New environment variable:** `TEALSTREET_NODE` (for example `01`) is the node's identity. It appears in `/health`, in every log line (including `order audit`) and in the session frame. It is identity, not a feature switch: every node runs the same code.

### Session frame and client resume

- **The session frame gains the node:** `session {resumeId, resumed, node}`. The web worker (`ExchangeServerWorker.ts`) stores `node` beside `resumeId`.
- **Reconnect with a `resumeId`:**
  1. The client first dials `wss://connect-tyo-<node>.tealstreet.io/account?resume=<id>&resumable=1`, with a 5s open timeout.
  2. `session {resumed:true}` continues as today.
- **Fallback:** the client uses a fresh `init` on `wss://connect-tyo.tealstreet.io/account` (no `resume`) and drops the old id when:
  - the per-node dial fails or times out;
  - it returns `resumed:false`;
  - it closes with 1013 (overloaded).
- **A resume that reaches the wrong node** (an old client, or the load balancer) finds no parked session. It gets `resumed:false` with a fresh `resumeId`, as today, and the client re-inits on that socket. The server never forwards sockets between nodes.

### Parked sessions on failover

- **When a node dies:** its live and parked accounts die with it.
  - Resting venue orders stay on the venue.
  - The client's fresh `init` on the survivor rebuilds the account from the venue, so open orders and positions reappear in the snapshot.
- **When a node is alive but unreachable** (a tunnel or network partition): its parked accounts would otherwise live out the 60s grace while the client re-inits elsewhere. Two measures cover this:
  - **Parked algos keep running (Sam's decision, superseding an earlier "parked algos hold").** Chases and TWAPs keep placing for up to the 60s grace; their orders need the browser's signature, so during the gap each waits up to the 15s signing timeout and then fails visibly. Grace expiry stops them and disposes the account.
  - **Evict on init.** Before building a private account, a node sends `POST /internal/evict {userId, accountName}` to each peer (`TEALSTREET_PEERS`, a node to private-address list).
    - The request is authenticated with an HMAC under `EXCHANGE_SERVER_PEER_SECRET` and is served only on the private interface, never through the tunnel.
    - The peer disposes any parked holder of that account, running algo shutdown first.
    - A peer that holds the account for a live socket replies `409 {node}`. The initiating node then refuses the init with `msg {op:'error', data:['account is active on node <node>']}` plus a `redirect {node}` hint. A client that gets the hint retries on that node's hostname.
    - An unreachable peer does not block the init; the bounded risk of not waiting is covered by the next point.
- **A bounded window remains.** If a peer is alive, holds a parked account, and cannot be reached for eviction, it can still hold that account for up to 60s. Its algos are held, so it places nothing. This is the accepted residual.

### Server-side algos on failover

- **Chases end** when their socket closes (unchanged), so a failover never moves a chase.
  - A chase's resting limit order stays on the venue as an ordinary open order.
  - The client shows it, and the user can cancel or re-chase.
- **TWAPs move only through the client.** The browser keeps its TWAP state and sends `twap:state` after the init on the new node, which restores schedules there; restore already skips ids it holds. There is no server-to-server algo transfer: the client's copy is the one record both nodes can see.
- **Duplicate risk.** `twap:state` restoring on a node while another node still runs the same TWAP is the only path to duplicate algo orders. Invariant 1 (load balancer failover, held parked algos, evict on init) closes it. Note that `twap:state` places orders, so it is a write: the `order audit` log must record it like `twap:start`.
- **Exec** commands are one-shot. A failover mid-command reports through the order's normal outcome, `unknown` when the completion was lost, and the client reconciles from the snapshot.

### Later: a shared lease

If active/active is ever wanted, the server should hold an owner lease per `(userId, accountName)` in Redis instead of relying on the evict call:

- The lease is `SET acct:<user>:<name> <node> NX PX 15000`, renewed every 5s.
- A node that loses its lease stops the account's algos at once and disposes it.
- `REDIS_URL` is already in the environment contract.
- This adds a dependency, so it waits for the Cargo.lock freeze to lift.

## Hosted method scope (claude-dazzy3)

A COSIGNED account's main-thread mirror forwards each engine read to this
server by its `callMethod` name, using the same list as LOCAL
(`rust.mirror-forwarding.test.ts`, SDK #2506).

- **Translated:**
  - `fetchOrderBook` (when the venue has `orderBookQuery`);
  - `fetchStpMode` (`stpControls`; private accounts);
  - `fetchLeverageForSymbol` as the engine's `fetchLeverage`. The mirror answers from its frame-fed rails first and asks only when they can't; a public account gets `null`.
- **Unsupported on hosted.** These are visible `UnsupportedMethod` refusals, never a guessed command, and are pinned by `options_stp_writes_and_rails_max_leverage_stay_unsupported_hosted`:
  - `fetchOptionChain`, `fetchOptionExpiries`, `fetchAccountGreeks`: options are out of scope;
  - `changeStpMode`: AsterDEX only, not a core venue;
  - `fetchSymbolMaxLeverage` with the `rails` source (Polymarket). The `market` source answers on the mirror.
- HyroTrader accounts are refused at init (`HYROTRADER_UNHOSTED`): their endpoints are browser build configuration.

## Not in v1 (after cutover)

- `exec` (hosted CLI), and the hosted chaser and TWAP. These run in the TS worker today; they need native ports.
- Shared public data across accounts. Each account engine runs its own public feed, and venue-side public sockets are cheap compared with a cutover risk.
- Slack alerts and the per-venue REST diagnostics in `/health`.

## Candle server

Placeholder for its owner, covering binary, port, health and env.


## DELEGATED action frames v1

The additive `order.action` family dispatches amend, venue cancel, cancel-all and
nuke on the admitted DELEGATED account using its own venue. `delegatedOrderActionsV1` follows
the existing feature/hosted-venue gate and `TEALSTREET_DELEGATED_ORDER_FRAMES=1` startup
activation. Each session captures activation at creation; changes require the
existing box drain/restart and fresh admission. Hosts also require the router's separate
`delegatedOrderActionLedgerV1` and the two original frame/ledger flags. Requests
bind UUIDv7 identity, version, action kind and target IDs in a separate canonical
fingerprint domain. Placement's four frames and fingerprints are unchanged.

Amend/cancel/cancel-all use the existing method translator, native command
completion and audit. Internal action dispatch carries the original deadline
through cancel-all symbol reads and command translation. Each native command
rechecks both expiry bounds and its captured registration after any startup wait and
immediately before dispatch. Expiry stays unknown without a notSent proof.
Nuke uses the account's existing native algo-host nuke,
awaiting cancellation and closing legs; a partial/interrupted action stays
unknown. Actor-queued nuke work carries the original registration and rechecks
its liveness, account holder and IO authority after load waits and after the
actor's store read. A server-owned account-port adapter also rechecks that
registration and both captured expiry bounds before every native command,
including close legs after an awaited cancel. Nuke receipts carry the original
monotonic age deadline and UUID expiry through startup and actor store waits;
neither wait resets the budget. Expiry blocks every remaining native leg and
returns unknown with an expired message, without inventing a notSent proof.
Retirement fences new legs and drops stale outcomes; already handed-off work
remains uncertain. An aborted/abandoned nuke settles unknown on
its captured output, never another owner's registration.

Typed action outcomes are `applied`, `refused` with a proven `notSent`, or
`unknown`. Multi-leg failures and errors lacking no-dispatch proof are unknown.
Every cancel-all leg settles through its shared completion group, including
pre-dispatch lease, capacity and command-ID refusals. A locally unsent leg
cannot refuse the whole action after another leg may have crossed physical IO.
Receipt counters share the existing registration lifecycle and size bounds.
`order.actionCancelQueued` belongs to the router and never issues a venue cancel.
Rejecting that frame uses the ordinary error envelope: it supplies no proof
about an earlier action and cannot fabricate an action refusal or negative ACK.
See `docs/plans/callers/delegated-action-frames.md` and the shared synthetic
`tests/data/delegated-actions-v1.json` contract.

## DELEGATED restart key status (codex-oqblq9)

The existing interruption marker also accepts `reason: "waiting-for-key"`.
Marked DELEGATED handover requests expose `delegated:status` waiting for a key;
only admitted handovers report key availability. This does not acknowledge a
restored job or create persistence. Client-held TWAPs retain the existing paused
restore/explicit-resume gate; lost chases and absent schedules cannot be rebuilt.
See [restart recovery](../../../docs/plans/callers/delegated-restart-recovery.md)
for the wire contract, scope and measured recovery limits.
