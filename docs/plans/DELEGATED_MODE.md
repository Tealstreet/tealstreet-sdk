# DELEGATED mode: options against spec v10

## Context

Status: discussion only. This document compares possible changes to **Rust
Runtime Architecture v10**, approved 2026-10-30 with rulings through 2026-11-01.
It does not approve a transport, retention period, forwarding interface, outbox,
or retry policy. The baseline below is the supplied substantive excerpt of that
spec, not a claim to have inspected its unavailable full page.

Scope: mobile manual writes through the server, HTTP versus WebSocket versus a
hybrid, longer key retention, outbox/idempotency/resend, routing reuse, and shared account identity/join validation. Existing
LOCAL and COSIGNED behavior is context, not an invitation to redesign them.

## Baseline (spec v10)

These are settled baseline points, not new proposals:

1. **Permanent modes:** LOCAL has browser-worker streams/algos; COSIGNED has
   server streams/algos and browser signing; DELEGATED has server streams, algos,
   and signing without an open browser. DELEGATED is phase 3, after cutover.
2. **Key storage:** permanent storage is browser-only. The temporary server copy
   is process-memory-only, never disk, logs, or swap.
3. **Handover:** per-account opt-in; the browser encrypts the key to a one-time
   server public key and sends it over the hosted connection, bound to user,
   account, and key. No expiry or extra key restrictions.
4. **Manual writes:** a full fed Rust page engine per account holds the key and
   sends manual order/amend/cancel/settings/close operations in one network step
   over the existing direct/proxy/tunnel route. A browser is open. Thin forwarding
   objects, thin objects plus separate signers, and stateless writers were rejected.
5. **Algo writes:** DELEGATED algos sign on the server and use the Tealstreet proxy
   fleet, without a browser.
6. **Last tab closes:** the server account and algos remain live; reopening
   attaches the page engine to that account.
7. **Revocation:** disabling DELEGATED, changing/deleting the key, or removing the
   account wipes the copy, stops its algos, and records what stopped.
8. **Restart:** copies are lost; affected algos visibly pause, “waiting for your
   key,” until another browser handover, unless the schedule lapsed.
9. **Custody:** one server holds a key copy, enforced by a per-account lease.
10. **Rejected key:** algos stop, the copy is wiped, and the user is notified.
11. **Exposure/audit:** compromise can expose memory-held keys; every
    server-signed order appears in the audit log.
12. **Mobile:** the WebView hosts the page engine and worker; native cannot run
    WebAssembly.
13. **App interface:** `getExchange(account)` returns the page engine; no facade
    or app migration.
14. **Unknown:** an order with unknown outcome is never automatically re-sent.
15. **Observability:** extend existing engine events, Requests Log, hosted verbose,
    diagnostic log, and server order audit; no new observability system.

The closest checked-in statement of #4 is “Manual writes are signed and sent on
the page” in [the runtime deletion inventory, lines 12–15][S1]; [the frame-fed
plan, lines 10–13][S2] also says the page engine sends writes itself. A literal
single-line “manual writes ... in every mode” was not found in the mirror's plan
files; #4 above supplies that baseline rather than inventing a quotation. The
server-forwarding options below would supersede it **only if approved**, and a
mobile facade would also reopen #13's explicitly rejected alternative.

## Current state (mobile and server)

Evidence snapshots: SDK `05241cd0f41b05c382fceed0e0347d9b81729a0c`; mobile
`8377a4f3f1e048df14ad76be93bf2837366bcd60`; router/roller source
`87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc`. These are source observations, not
live deployment verification.

| Observation                                                                         | Evidence and implication for a delta                                                                                                                                                                                                                                                                                                                                        |
| ----------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Mobile already selects hosted exchange mode.                                        | [Mobile workerFactories, lines 5–12 and 173–175][M1] describes the WebView Worker host and returns `true` for hosted mode. Hosted does **not** mean no client engine/signer.                                                                                                                                                                                                |
| A hidden WebView owns the exchange socket, signing, and sessions.                   | [ExchangeHostWebView, lines 30–40 and 124–141][M2] loads the exchange-host page. [exchangeHostBridge, lines 10–25 and 79–103][M3] documents signing in the Worker and process-loss handling.                                                                                                                                                                                |
| Mobile's SDK copies are generated inputs, not a verified complete vendor tree here. | [Babel aliases, lines 6–9 and 51–59][M4] resolve safe-cex into the synced source tree; [patch-config, lines 490–529][M5] preserves the mobile worker factories. This checkout lacks generated safe-cex/hosted-client implementations, and its checked-in vendor entry is Tealchart. A deployed exchange-host bundle or complete mobile SDK copy was not inspected.          |
| A fed SDK mirror still loads an engine and signing host.                            | [rust.exchange.ts, lines 1183–1202][S3] calls `loadWasmEngine` and `ensureSigningHost`. Its current delegation-related signer names alone do not prove the proposed server key-custody lifecycle exists.                                                                                                                                                                    |
| The reviewed native server still constructs private accounts with remote signing.   | [accounts.rs, lines 1125–1153][S4] asserts private accounts have a remote provider and labels them `hosted_cosign`; [remote-signer setup, lines 71–77 and 133–141][S5] rejects a nonempty secret. Server-held-key construction would be real implementation work.                                                                                                           |
| Current admission starts with verified user identity.                               | [http.rs, lines 792–825][S6] verifies the proxy token and reserves a user socket; [auth.rs, lines 89–104][S7] checks RS256, expiry, and nonempty user ID. An account name supplied by a caller is not owner authority.                                                                                                                                                      |
| Deferred account creation already carries live authority.                           | [accounts.rs, lines 380–393][S8], [946–966][S8-init] and [1125–1195][S8-placement] captures init incarnation plus lease before pacing, checks after pacing and at actor placement, then installs the transport I/O guard. [router_lease.rs, lines 137–193][S9] compares the captured lease generation.                                                                      |
| #3391 adds stable owner identity, not an interchangeable live-session fence.        | At review time [PR #3391][P3391] is open at `ec49cdd8159600a23baf60afb344cad5b50fc119`, not in this SDK baseline. Its [twap_owner.rs, lines 5–45][O1] derives generation from authenticated user identity, stable across rebuilds/boxes. Its [accounts.rs binding construction][O2] is server-owned. Adoption is a dependency to assess, not something this document lands. |

For either write transport, a possible binding would combine the authenticated
user, account/key incarnation, connection or registered HTTP signing identity,
and the existing **live** owner lease/boot authority. The #3391 owner-derived
generation could identify whose restored work it is; it could not establish that
an old A connection/job is still current after A→B→A. Capturing live authority
before each queue/await and rejecting stale work at execution would reuse the
current guard pattern rather than refreshing an old job's authority.

## Account identity

**Spec says:** key custody is tied to user/account/key (#3), reopening attaches
to running work (#6), explicit revocation stops that account (#7), restart loses
the key (#8), and a per-account lease prevents duplicate custody (#9). **Decided shared join principle; details still design:** identify a COSIGNED or DELEGATED account by `(verified Tealstreet
user ID, public API key ID)`, with account name only a display label. This would
standardize sharing for two saved accounts belonging to the same user and using
the same exchange key. The shared identity principle is decided; canonicalization, custody and migration details below remain design work.

Cross-user key sharing is out of scope.

### Today's sharing and the rebuild risk

Today [accounts.rs:449][I1] stores accounts by a pair of strings;
[rebuild_refusal:911–943][I2] spells that pair as `(user, account_name)`.
[build_fingerprint:708–744][I3] includes venue, route, and settings, hashing key
and passphrase with OS-random salt held in a process-local `OnceLock`. The salt
is neither a stable key identifier nor shared between boxes.

[obtain:1013–1038][I4] reuses an identical fingerprint. A differing fingerprint
can rebuild when no other live holder blocks it; it disposes the previous account
before building a replacement. The [stop path:1375–1410][I5] also handles algos.
Thus “no live holder always rebuilds” would be too broad: **same fingerprint
reuses; different fingerprint with no blocking live holder can dispose/rebuild**.
The DELEGATED risk is real structurally: under #6 the client may be absent while
a key and TWAPs remain live, so reusing a display name with a different key could
otherwise dispose unrelated running work. The current source is COSIGNED [S4];
this is a risk of extending its name-keyed behavior to retained DELEGATED keys,
not a claim that a deployed retained-key feature was reproduced.

### Identity deltas and trade-offs

| Spec says                                                      | Proposed option                                                                                                                                               | Costs/risks and unresolved boundary                                                                                                                                                                                                                                                              |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| User/account/key tie (#3); one custodian (#9).                 | Define public key ID by a reviewed per-venue canonicalization rule, then make `(uid, canonical key ID)` the sharing identity.                                 | Venue/environment namespace, account/subaccount, API-key slot and key type may need encoding inside the canonical ID. Case-folding opaque API IDs could merge different keys; aliases and rotated agents could accidentally merge signing authority. Public ID alone provides no authentication. |
| The same account remains live when clients leave (#6).         | Same key reattaches to its existing engine/algos, even through a different display name; another key creates a separate identity, never implicit replacement. | Name→identity mapping, duplicate display labels, subscriptions, order attribution, UI stores and existing client IDs need migration. This is additional scope beyond simply changing a map key.                                                                                                  |
| Disabling/changing/deleting/removing triggers revocation (#7). | Explicitly target the affected key identity for replacement/revocation.                                                                                       | If two saved labels share one identity, deleting one label versus revoking the shared key are different user actions; “account removed” in #7 needs an explicit semantic mapping. A rename would not revoke. No implicit disposal of an unattended different key would occur in this candidate.  |
| Restart loses venue keys, and work waits (#8).                 | A stable salted identity digest could survive restarts while venue secrets remain volatile.                                                                   | Identity continuity would not recreate signing authority or auto-resume jobs. Persisting/recovering identifiers and outcome metadata is distinct from persisting venue secrets.                                                                                                                  |
| One key copy per server lease (#9).                            | Keep all of a user's key identities on their existing user-router box, or route/lease each key identity independently.                                        | User-co-located keys reuse today's user custody and drain proof, but still need per-key account custody. Per-identity placement adds routing keys, lookup, independent leases/proofs, multi-key drain/roll complexity and migration; it is not supplied by today's user-level DO.                |

Per-venue ID questions are part of this delta, rather than something answered by
the generic string `key`. For ordinary API-key venues, the candidate public API
key identifier could be normalized with venue/network scope. For **Hyperliquid**,
main account/vault address and authorized agent/API-wallet address are different
concepts; using only the main address could conflate separately rotated agents.
For **Lighter**, L1/public address, resolved account index, API-key index and API
signing key are distinct. For **Paradex**, account address, Ethereum wallet,
Stark signing public key and subkey/agent address may differ. The option structs expose key/credentials and vault fields for Hyperliquid
[I6], L1 address/identity/API-key index for Lighter [I7], and account/subkey
addresses plus an Ethereum-derived credential flag for Paradex [I8]. Lighter
also resolves account and API-key indices for authenticated reads [VL2]. These
fields do not establish one canonical ID for every mode. A per-venue mapping would resolve network, wallet
role, key slot and rotation before sharing an engine. No secret or display name
would become the identity in the proposed model.

A stable server-held digest is another consideration: for example, a versioned,
domain-separated HMAC over verified uid plus canonical public key ID, using a
server-held pepper shared by the router and boxes through existing protected
configuration. That pepper would be durable configuration material, **not** a
persisted exchange API secret (#2). A non-secret stable salt is simpler but offers
less protection against enumeration of public IDs. Either approach introduces
distribution, versioning and rotation work; today's per-process salt cannot
serve it. Rotation could fence admissions and explicitly remap ownership/outcome
records under a coordinated version change. Uncoordinated dual ID namespaces
could create two custodians, so rotation is not just an independent restart.
After a restart the stable digest could identify whose account is waiting under
#8, while the key itself would still need encrypted handover. This remains an
option, not a mandate to introduce persistent key storage or a new secret system.

The same key with different settings could preserve today's refusal while another
live holder exists [I2]. The current Hyperliquid builder-only reuse exception
([obtain:1022–1033][I4]) would be preserved or explicitly reassessed, not silently
removed. When no holder exists, changing settings on a running DELEGATED identity
would still need a separate, explicit operation: reattach would not implicitly
dispose its engine, retained key or TWAPs. Sharing one key between COSIGNED and
DELEGATED would also need a signing-mode transition rule; “every key can use both”
does not settle simultaneous signer custody, mode precedence or revocation scope.

### Authentication on attach

For the proposed identity, the public `(uid, key ID)` tuple is a lookup key,
**never an attach credential**. The requested authentication boundary is a real
verified Tealstreet JWT on each connection/attach/rejoin/resume; uid comes from
the verified token, not an init field. The options below assess additional secret
proof; they do not make a public ID sufficient.

The [exchange-router README, “How it routes”:10–14][A1] documents RS256 verification
before trusting `userId`, with box verification again. Code confirms:

| Path                                                                              | Current verification coverage                                                                                                                                                                                                                                                                                                                                                                                               |
| --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| New authenticated connection, including a new socket attaching an existing engine | Worker [index.ts:38–41][R1] calls [token.ts:41–71][A2], which verifies RS256 and expiry before extracting uid. Box [http.rs:792–812][S6] independently verifies the token. An invalid/absent router token falls through toward a public box, not authenticated key attachment; the box independently rejects invalid supplied tokens and restricts unauthenticated init/messages [A3].                                      |
| New socket rejoin or COSIGNED 60-second resume                                    | Same upgrade verification, not a first-init-only shortcut. [Session::open:373–404][A4] takes uid/token from the verified `Identity` and calls resume/takeover. [accounts.rs:1793–1839][A5] matches the parked/open session's user before transferring holders. The resume ID is not standalone auth. The server's configured resume grace, rather than router README's 90-second placement preference, governs parked work. |

Security findings on the current server: private, Tealstreet Linear doc "Tealstreet migration", section COSIGNED/DELEGATED join.

Token refresh options include forced reconnect with a fresh JWT, naturally
reusing both verifiers and resume, versus explicit in-band reauthentication that
verifies the new token, keeps uid unchanged, updates the expiry deadline, and
revokes old connection-generation work. In-band auth does not exist in the
inspected `Inbound` handler; it is additional protocol work. Whichever is explored,
an expired authenticated connection and the account's independent DELEGATED algo
authority are different lifecycles (#6/#7).

### Decided join principles; protocol details still design

The shared authentication/join principle is decided: COSIGNED and DELEGATED use
JWT on every connection/rejoin, identity `(verified uid, key ID)`, routing to
the engine custodian, and the same admission/rejoin semantics. DELEGATED differs
by encrypted key handover, volatile caching, and server signing; COSIGNED does
not receive the secret on the server. Transport, 48-hour retention, and outbox
deltas elsewhere remain options. Public-key normalization and migration details
are still design work.

STEER 11 establishes one shared authentication/admission flow for both modes.
Its initial client-cosigned-in-both-modes wording is superseded by the later
STEER 12 and STEER 13 signing refinements: the joining client signs in COSIGNED;
the server signs with the joining connection's encrypted provided pair in
DELEGATED. These later refinements govern the flow below. They preserve the
shared venue-validation principle without requiring a client venue signer in
DELEGATED. They do not select any transport or retention/outbox option.

The refined join direction is one **server-executed venue validation** before
multiplexing a connection into a running account:

1. Verify the Tealstreet JWT and derive uid, resolve key identity/custody, and
   create isolated pending-connection authority.
2. Construct the exact approved private balance/account validation request(s),
   with current venue freshness fields and the claimed key identity.
3. **COSIGNED:** send the validation sign request exclusively to the **new**
   pending connection; its client signs, and the server executes the constrained
   signed HTTP request against the venue. It is not sent through an existing
   account signer.
4. **DELEGATED:** every new connection supplies key plus secret through spec #3's
   one-time-public-key encrypted handover, even if the key is already running.
   The server signs the validation request with that **provided pair**, not the
   cached pair, and executes it. This refines the earlier client-cosigned-both
   suggestion: the shared check is identical except for who signs.
5. Only authenticated venue acceptance for the provided identity admits the
   connection. Pending clients see no account frames, have no write capability,
   and do not become the account signer. Refusing commands while pending avoids
   silently building an executable pre-auth queue; an explicit bounded intent
   queue would need separate admission and original-owner fencing.

The current [Account::hold/sign_through:268–297][J1] immediately selects the newest
holder and calls `link.attach`; therefore validation cannot first call `obtain`,
`hold`, resume holder transfer, or `Session::join` and then verify. A candidate
implementation would create a **separate pending SignerLink bound to that socket**,
reuse the existing remote-sign request/response parser and exact request builder,
and destroy the link on failure/disconnect/expiry. [SignerLink::new:73–90 and
round_trip:145–183][J2] bind the destination socket; [resolve:212–232][J3] checks the
responding socket. Pending sign responses would be routed to this pending link,
not the existing joined-account loop at [session.rs:434–439][A3]. The account's
newest-device signer routing would only change after successful admission.
DELEGATED would use the same pending state machine with a short-lived candidate
signer holding the supplied decrypted pair, never borrowing the resident copy.

#### Reusing validateAccount: current location and per-venue feasibility

[ValidateAccountResult:117–122][V0] exposes `error`, optional balance, and typed
failure kind; [base.ts:1008–1010][V1] is an unimplemented interface default.
The Rust-backed TS implementation at [rust.exchange.ts:2511–2575][V2] starts the
engine and waits for balance-loaded/startup failure, with a timeout set by the
[45,000 ms constant at line 515][V2-timeout].
It is **not a primitive single validation HTTP request**, and a preloaded balance
does not freshly prove the joining client signed anything.

`validateAccount` is explicitly in `WORKER_MIRROR_UNSUPPORTED_METHODS`
([rust.exchange.ts:388][V3]); [refuseOnMirror:1882–1887][V4] replaces it with an
unsupported refusal on a main-thread worker mirror, even though that mirror can
also have a fed page engine. An owning/non-mirror RustEngineExchange (such as
the worker/in-process validation instance) can run the TS method. The native
server host does not expose that TS function through its `methods.rs` command
translation. Reuse therefore means extracting the relevant **native private-read
request and acceptance classifier** into the shared server join-validation flow,
not forwarding the current TS method name or running a full new engine/startup
against an already-admitted account.

The table is a source-guided request inventory, **not certification that every
row already has a server-hosted cosign profile**. The TS reference implementations
describe existing credential-check intent; native freshness, acceptance and remote
signing support would be verified before exposing a join path. Optional best-effort
UI balance reads need not become proof requests. A read using a secondary key
does not prove possession of the primary trading secret.

| Venue/family               | Existing check and candidate server validation request(s)                                                                                                                                                                                                          | Single client-signable private proof?                                                                                                                                                                                                                                                         |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Binance futures/inverse/PM | Primary `ENDPOINTS.ACCOUNT`; if a secondary key is configured, the same account request signed by that key too. Futures [V5-futures], inverse [V5-inverse], and PM [V5] each implement this check; PM's extra balance fetch is best-effort display, not the proof. | One per key, **two for primary + secondary**; each bound to its credential role. No “one request for all keys” claim.                                                                                                                                                                         |
| Binance spot/options       | Spot account endpoint then optional positions/balance [V6]; options uses balance [V6-options].                                                                                                                                                                     | Candidate one private account/balance proof per relevant key; separate endpoint/profile for each product.                                                                                                                                                                                     |
| Bitget/inverse/USDC        | Account-type discovery then `fetchBalance`; the code explicitly treats the BALANCE call as the key check. [V7]                                                                                                                                                     | Balance can be the single proof once account type is known; cold account-type discovery can add requests.                                                                                                                                                                                     |
| Bybit/inverse/spot/demo    | Account-margin/config check; display balance is best-effort: standard [V8], inverse [V8-inverse], spot [V8-spot]. Demo inherits the standard implementation [V8-demo].                                                                                             | Candidate one private account/balance check with correct region/product/demo routing. HyroTrader is currently refused by server configuration [I9], not silently admitted through Bybit's proof.                                                                                              |
| BloFin                     | BALANCE response validates and supplies balance. [V9]                                                                                                                                                                                                              | Candidate one private balance read.                                                                                                                                                                                                                                                           |
| Bitrue                     | ACCOUNT then optional balance/positions. [V10]                                                                                                                                                                                                                     | Candidate one private account read; optional display reads excluded.                                                                                                                                                                                                                          |
| Bitunix                    | ACCOUNT with account params, then optional balance. [V11]                                                                                                                                                                                                          | Candidate one private account read with native success-code validation.                                                                                                                                                                                                                       |
| BingX/spot                 | Futures validates through the primary balance-data fetch [V12]. The separate spot fixture reads its balance endpoint [V12-spot-balance], [V12-spot-paths]; it is not a validateAccount implementation.                                                             | Candidate primary private balance read; spot/futures endpoint and signing profiles evaluated separately.                                                                                                                                                                                      |
| MEXC                       | `fetchBalance`. [V13]                                                                                                                                                                                                                                              | Candidate private balance path; a key-only/public-data configuration would not prove the secret.                                                                                                                                                                                              |
| OKX/EU/demo                | BALANCE for standard profile [V14]; ACCOUNT_CONFIG for EU profile [V14-eu].                                                                                                                                                                                        | Candidate one private read per profile, including passphrase and freshness.                                                                                                                                                                                                                   |
| KuCoin futures/spot        | Futures `/api/v1/account-overview` [V15]; spot ACCOUNTS then optional display balance [V15-spot].                                                                                                                                                                  | Candidate one private account/balance read per product, including passphrase/signing role.                                                                                                                                                                                                    |
| Kraken futures/spot        | Futures ACCOUNT [V16]; spot private BALANCE, then optional display balance [V16-spot].                                                                                                                                                                             | Candidate one private request with the relevant key's nonce. Existing nonce ownership/order constraints would remain significant during simultaneous joins.                                                                                                                                   |
| Phemex                     | ACCOUNT with the configured collateral/account scope. [V17]                                                                                                                                                                                                        | Candidate one private account read with signed expiry.                                                                                                                                                                                                                                        |
| WOO                        | Primary ACCOUNT and optional secondary ACCOUNT, then best-effort display balance. [V18]                                                                                                                                                                            | One per key; can be **two**, with no proof borrowed from the other credential.                                                                                                                                                                                                                |
| Crypto.com                 | Private BALANCE request; optional display fetch. [V19]                                                                                                                                                                                                             | Candidate one signed private read, with native body/status validation.                                                                                                                                                                                                                        |
| WEEX                       | ACCOUNT_CONFIG with permission checks, then balance/positions. [V20]                                                                                                                                                                                               | Candidate one private config/balance check; a venue response reporting trading disabled does not admit a trading session.                                                                                                                                                                     |
| Backpack                   | CAPITAL read followed by optional balance. [V21]                                                                                                                                                                                                                   | Candidate one signed private read; read-key success alone would not prove a distinct primary trading key.                                                                                                                                                                                     |
| Orderly                    | ACCOUNT private read. [V22]                                                                                                                                                                                                                                        | Candidate one private request after account/signing-key identity setup.                                                                                                                                                                                                                       |
| Coinbase/spot              | Legacy validation uses MARKETS with best-effort balance. [V23]                                                                                                                                                                                                     | Market/catalog success is not sufficient evidence without proving it is authenticated. A fresh private accounts/balance request with the intended JWT signing key would be the candidate; not a public catalog.                                                                               |
| ApeX Omni                  | USER endpoint plus local OmniKey-seed setup. [V24]                                                                                                                                                                                                                 | Private REST acceptance can prove the REST credential, not by itself the separate OmniKey trading seed. Additional validation design remains unresolved.                                                                                                                                      |
| AsterDEX                   | getAccount plus API-version/wallet credential checks. [V25]                                                                                                                                                                                                        | Candidate authenticated account read after v1/v3 wallet/profile resolution; not interchangeable key schemes.                                                                                                                                                                                  |
| Paradex                    | SDK/auth initialization then ACCOUNT. [V26]                                                                                                                                                                                                                        | Multi-step: fresh client-signed auth, then private account read tied to that session. A cached JWT alone would not establish the new client's secret.                                                                                                                                         |
| GRVT                       | Authenticate, then funding-account summary. [V27]                                                                                                                                                                                                                  | Multi-step session/API-key authentication; account acceptance alone does not demonstrate use of the wallet trading secret.                                                                                                                                                                    |
| Hyperliquid/spot           | Perpetuals [V28] and spot [V28-spot] validators locally check wallet/private-key consistency; state reads are not that proof.                                                                                                                                      | **No equivalent single signed private balance proof established.** A public account read cannot implement the decided join check. Venue-compatible server-executed proof needs further design.                                                                                                |
| Lighter                    | Legacy account lookup/API-key-index validation and optional local crypto initialization [V29]. Native authenticated `GET /api/v1/accountActiveOrders` with query auth is a non-mutating candidate [VL1].                                                           | Candidate one signed auth-token operation plus a private orders read after account/key-index resolution [VL2], [VL3], [VL4]. Remote signing is enabled for Lighter and Lighter-Robinhood [V31]. Acceptance, freshness and suitability as the shared join check remain design work; see below. |
| Polymarket                 | Derive API credentials if absent, then balance allowance. [V30]                                                                                                                                                                                                    | Potential multi-step wallet-to-API credential proof; native construction currently declares remote signing disabled [V31]. Not currently a generic server-cosign join path.                                                                                                                   |
| TxFlow                     | No dedicated TS reference validation found in this inventory; registry declares no private eligibility [V31].                                                                                                                                                      | No supported private balance proof established. Public account state is not validation.                                                                                                                                                                                                       |

Lighter's native candidate resolves the account index and API-key index into an
`AuthRecipe`, then dispatches through `request_signed_managed` [VL2]. It signs
`deadline:account_index:api_key_index` [VL3]; the sign request uses SchnorrDigest
with a deadline 60 seconds after the dispatch timestamp, rounded to seconds
[VL4]. The resulting token is inserted into the query/header [VL5]. Existing
source tests cover query and header token placement [VL6]; they are not join
admission tests or live venue verification. This is distinct from the unsigned
account snapshot and legacy local precheck.

As a join option, that private orders read could replace a balance read only if
its authenticated acceptance is approved as equivalent validation. Account/key
mapping, response classification and pending-connection isolation still need
specifying. The token's seconds-resolution deadline does not make each join's
proof unique; token reuse within its validity window needs an explicit freshness
assessment. One token-signing operation and one private HTTP read could suffice
once identity is resolved; discovery adds requests. No candidate-token reuse
across joining connections is assumed. Roll storms would therefore consume
venue read capacity per connection, and failure would use the same fail-closed
pending/refusal behavior below. This identifies an existing native request
candidate, not a completed join validator or an additional trading mutation.

For a normal HMAC/private read, the server would own the exact venue, method,
path, query/body and fresh native timestamp/nonce/expiry, send it only to the new
connection in COSIGNED, then execute the returned signing result with the
existing constrained transport. It would not execute an arbitrary client URL,
accept a client “validation succeeded” boolean, or trust cached account state.
The fresh response would be bound to the pending attempt, identity, credential
role, live owner lease and connection generation.

Freshness analysis also distinguishes _fresh_ from _unique_: timestamp-only signatures
can be replayable within the venue's acceptance window, and seconds-resolution
timestamps can collide across joins. The server's one-use pending sign request,
monotonic/native nonce rules where supported, a non-reused exact signing preimage,
and venue rejection of stale requests would need assessment per venue. Merely
placing a random join ID outside the signed venue payload would not bind it.
No invented nonce parameter or mutation would be injected into a venue balance
request to force uniqueness.

Cost model: for `r` sequential validation requests, COSIGNED join adds roughly
`r × (Dcs + Dsv)` beyond the live connection; DELEGATED adds
`r × Dsv` after the provided-pair handover, plus local signing. Authentication or
discovery can add more, and a one-time encrypted handover has its own setup cost.
For 1,000 reconnects and two proof requests per join, that is **2,000 venue
requests**, before discovery/normal startup, not 1,000 cheap local checks.
Per-identity validation admission, bounded queues and jitter could limit a roll
storm; one successful client's proof would not silently admit other connections.
Timeout/rate-limit/unreachable/invalid-key responses would fail closed for the
pending connection, preserve the running account and show pending verification
then a typed “Could not verify your key; account not attached” refusal. A failed
read is not proof of an invalid key; a venue timeout would not automatically
revoke the running account under #10.

The same validation would apply on rejoin and the 60-second resume. It could
consume part or all of the resume grace during an outage; extending that grace,
or carrying a prior proof through it, would be separate design choices rather
than bypassing validation. No background/roll fallback to another holder's
signer, cached DELEGATED copy or JWT-only admission is part of the shared check.

#### DELEGATED provided-pair lifecycle and rotation

Every new DELEGATED connection would hand over its candidate pair using #3's
encryption to a one-time server public key. The decrypted candidate would live
only in pending, memory-only state until venue validation completes, bound to
the connection/identity/owner generation. Failure, timeout, disconnect or
supersession would immediately wipe that candidate and its signer references;
it would never enter the resident cache or affect the existing key/algos.
Validation using the **resident** secret would prove only that the box can sign,
not that the newcomer supplied the right pair.

Same key ID with a venue-valid **different secret** is a rotation question,
not an automatic cache replacement. Two design options against #7 are:
(a) refuse attachment as a credential-incarnation mismatch and ask for explicit
rotation, leaving existing work running; or (b) perform explicit #7 revocation,
record/stop the old account's algos, wipe old signer references and queued work,
then install the newly validated handover as a new credential incarnation.
Resuming stopped work would be explicit, with Unknown effects preserved.
Venue validity alone would not select option (b). Different key IDs never
replace a running key implicitly under the identity direction; rotation of the
same key ID deserves equivalent explicit intent. Revoking one shared identity
also affects its other saved display labels/connections.

After restart, #8 already entails re-handover, so the venue validation could ride
on that exchange rather than add another secret-transfer ceremony. It still
adds venue calls during roll storms, and a failed new pair would leave work
waiting for a key rather than destroy evidence. A constant-time comparison with
the cached secret is a cheaper **considered fallback only**; it does not prove
current venue validity and is not the selected join-check direction.

#### Earlier proof alternatives (considered, not the join direction)

Earlier options included DELEGATED nonce-HMAC, COSIGNED JWT-only attachment,
a venue read, and registered nonce-bound verifiers. The venue-read direction
above supersedes JWT-only or generic local verification as the admission rule.
A direct DELEGATED nonce-HMAC could be a cheaper optional extra, but is not
needed for the decided validation flow. For HMAC venues, storing `H(secret)`
cannot verify an HMAC under the unknown raw secret; if the response uses
`H(secret)` as its HMAC key, that digest becomes a stealable attach credential.
Its leak can allow attach impersonation without necessarily revealing the raw
venue secret. Registration itself would still need authentic possession proof.

Asymmetric wallet/Ed25519/RSA keys allow a nonce signature to be verified against
an authenticated public key without retaining a private key; HMAC keys do not.
That remains a considered alternative for the weak/no-private-read cases, not an
unapproved replacement for the server-executed venue-validation direction.
Public-address/index-to-signing-key mapping remains venue-specific. JWT identifies
uid; a secret proof alone does not. JWT alone does not meet the decided validation
principle in either mode. Proof freshness and key presence do not replace JWT,
current owner authority, or pre-admission isolation.

## Option 1: SDK writes over per-request signed HTTP

**Spec says:** manual writes originate in the page engine (#4/#12/#13), while the
server's retained key signs algo writes (#5). **Possible change:** a mobile SDK
HTTP writer would submit typed manual operations to that server account instead.
The existing encrypted hosted key handover (#3) could remain unchanged.

### Authentication, replay, and owner binding

One candidate is a versioned JSON write API over TLS, authenticated by the current
user token **and** proof of possession of an app request-signing key registered
during the authenticated encrypted handover. This is not an exchange API secret
and would not reuse that secret as an HTTP MAC key. A candidate signature input
would length-prefix the protocol version, HTTP method, authority, path/query,
exact body digest, user/account binding, key incarnation, server boot/session
challenge, request sequence, and logical idempotency key. An implementation
choice of native or JavaScript signature support remains open; this option has
an on-device **request-authentication signer**, even if it removes venue signing
and WASM.

Session challenges and monotonically checked per-device sequences could avoid
depending on the phone's wall clock. A bounded replay window could permit
concurrent requests without silently accepting repeated sequence numbers;
duplicates would consult the logical outcome record rather than execute again.
Server-clock submission age is a separate outbox question below. Existing token
expiry and router clock-skew checks could remain unchanged; this is not a proposed
new exchange-key expiry rule. Challenge loss/restart would call for fresh
authentication and reconciliation, not acceptance of old signatures.

Trade-offs: per-request proof travels independently of the key-delivery socket,
which helps APIs and reconnects but adds canonicalization, key registration,
replay state, and key-loss handling. A stolen token alone would not authorize a
write under this candidate; compromise of the app signing key or live app still
would. A signature authenticates intent, not exchange acceptance.

### Routing and failures

**Spec says:** custody is single-server per account (#9). **Possible change:** add
an authenticated HTTP write/status branch to the existing per-user router,
forwarding to the **existing custodian** without independent placement. Today
[router index.ts, lines 18–43 and 61–73][R1] sends verified `/account` WebSocket
upgrades through `toUser`; ordinary HTTP goes to a default box. HTTP owner routing
is therefore **not already implemented**. [UserRouter's selection and custody
flow][R2] and [proof gate][R3] are reuse candidates, but the [upstream response
handling expects an upgrade][R2-upgrade], so this would require a separate HTTP
forwarding path inside the same ownership authority. Copying only its home-box hash would not preserve custody.

Extending that router could avoid a second routing system, at the cost of adapting
owner admission, per-account key availability, drain/proof behavior, boot checks,
body limits, cancellation, and responses. Without this extension, a separate
routing system is an explicit cost and split-custody risk of this option; an
arbitrary load-balanced HTTP fallback would not be equivalent.

Connection failure after submission would leave a possibly issued write Unknown;
HTTP timeout/502 alone would not establish `notSent`. A duplicate/status request
could return the original result. A new signature or new TCP connection would
not grant permission to reissue an Unknown. The proposed reconciliation policy
below applies equally to HTTP and WebSocket; #14 remains the baseline.

## Option 2: writes over the authenticated hosted WebSocket

**Spec says:** the hosted connection already delivers encrypted key material
(#3), but manual writes originate at the page engine (#4/#12/#13). **Possible
change:** extend that authenticated connection to carry mobile write commands,
bound to its server-derived user/account/key grant. No additional per-order
signature would be needed in this candidate; TLS and authenticated session
custody provide the transport authentication. WebSocket itself is a TCP-based
framing protocol, not an application authorization scheme. [RFC 6455, §§1.2, 4][WS]

A candidate message would carry protocol version, server-issued connection ID,
account/key incarnation, monotonically increasing connection sequence, immutable
logical idempotency key, operation, and typed arguments. The server would derive
the user from the verified session, then compare the carried binding with that
session and the current live lease before dispatch. Connection sequence would
detect replay within the socket; the shared idempotency record would cover
different sockets/reconnects. No phone timestamp is necessary for sequence
validation. Age checks and server venue-signing clocks are separate concerns.

### Same-connection provenance: advantage and limit

The socket delivering a key and then its orders provides a useful provenance
chain: the accepted key grant and write ingress share one authenticated channel,
without a per-request signing credential that can be copied and replayed later.
This is an argument for assessment, not stronger authentication by definition:
session-token theft before handover and compromise of the live client/server
remain relevant. Baseline #11's server-compromise exposure remains unchanged.

Under the refined DELEGATED join direction, every new connection would deliver
its own encrypted candidate pair and complete venue validation before attaching.
That preserves a same-connection handover/validation chain even when the resident
engine is shared. A fresh grant would refer to the validated key incarnation;
merely knowing its public ID would not admit a second device. A failed handover
would not let a newcomer borrow the resident key or signer.

### Reconnect, suspension, and flow control

The existing router observes all of a user's sockets and their actual owner
([UserRouter, lines 52–78][R4]), persists custody before forwarding, and renews
that owner's lease ([lines 365–408][R2]). Reusing this path preserves an existing
placement authority rather than adding one, subject to #9's finer per-account
custody check. A successful socket reconnect alone would not prove key presence.

On Wi-Fi/mobile transitions or background suspension, a candidate resume would
re-authenticate, bind a fresh connection generation, verify owner/key presence,
fetch outcomes for pending logical IDs, and then consider eligible queued work.
An order sent before a lost acknowledgement would display Unknown pending
reconciliation; sending a new socket message would not mean a second order.
Baseline #8 governs a restarted box unless explicitly changed. A different box
has no memory-held key merely because the router assigned it, and would need a
new encrypted handover; automatic key transfer is a separate option, not implied.

TCP retransmission can block later WebSocket bytes, and large market-data frames
can delay order acknowledgements. Candidates include bounded frame sizes,
prioritized control/result queues, and asynchronous account execution that does
not await a venue result in the socket receive loop. These mitigate application
backlog, not TCP head-of-line blocking. A separate control socket reduces shared
data backlog but loses the literal same-socket provenance argument and still
uses the same owner/key grant.

## Comparison table

All rows describe options against #4/#12, not approved behavior.

| Dimension              | Signed HTTP writes                                                                                                      | Hosted WebSocket writes                                                                              | Hybrid                                                                       |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Authentication         | Existing user authentication plus registered per-request proof; extra signing/replay implementation.                    | Existing authenticated connection plus per-message binding/sequence; no new per-order signature.     | Two ingress mechanisms and one shared binding/outcome authority.             |
| Provenance             | Write proof bound to the handover grant; write need not use its connection.                                             | Strong direct chain on the delivering socket; attaches become same-owner grants.                     | Write fallback inherits HTTP proof and loses literal same-socket provenance. |
| Routing (#9)           | Existing owner router could be extended; default HTTP routing is insufficient. Otherwise count a second routing system. | Existing user custody/pins/proofs reused, with per-account custody retained.                         | HTTP owner-routing work remains even if used rarely.                         |
| Warm write latency     | One client/server RTT plus one server/venue RTT, plus local proof/validation.                                           | Same network RTT count, smaller framing and no per-write proof.                                      | Same per chosen path; querying status first adds a client/server RTT.        |
| Connection disruptions | Independent requests ease recovery; timeout still does not prove non-delivery.                                          | Shared channel reduces setup while healthy; reconnect and phone suspension interrupt all operations. | More recovery choices, plus races/duplicate ingress to resolve.              |
| Head-of-line pressure  | Concurrent HTTP/2 streams can separate application responses; shared TCP can still block.                               | Shared byte stream can put data ahead of control/results.                                            | Extra connection can isolate data backlog, at operational cost.              |
| Key-loss/restart       | Neither request signature nor HTTP routing restores the memory-held venue key.                                          | Neither session resume nor owner pin restores that key.                                              | Neither fallback path restores it.                                           |
| Unknown (#14)          | Original outcome lookup; no baseline automatic reissue.                                                                 | Same; a fresh socket sequence does not authorize reissue.                                            | Same record across transports; fallback is not a retry bypass.               |
| No on-device signer    | Removes venue signer only; request-authentication signer remains.                                                       | Could remove venue signer without adding a per-request signer.                                       | Depends on whether HTTP writes are included.                                 |

Latency estimate, not a benchmark: let `Dcv` be a warm client/venue RTT over the
account's existing route (possibly proxy/tunnel), `Dcs` client/router/server RTT,
and `Dsv` server/venue RTT over the intended egress. Spec #4 manual writes are
approximately `Dcv`; either forwarding option is `Dcs + Dsv + queue/CPU`. Example
assumptions `120 ms`, `80 ms`, `10 ms` respectively give roughly `120 ms` versus
`90 ms`, not a guaranteed improvement. A remotely co-signed server request adds
about another `Dcs` for sign-request/response, giving `170 ms` in that example;
that comparison is to COSIGNED server work, not today's direct manual write.
Client→router→box→venue adds service legs versus direct client→venue; the router
leg is already included in `Dcs`. Cold DNS/TCP/TLS/upgrade and handover can add
multiple RTTs for either option. No latency benefit is assumed for cold start.

## Options assessment (for discussion)

One discussion leaning is **Option 2**, if server-forwarded mobile writes are
the desired delta: it reuses the hosted custody path and avoids app request-key
management. This is not a choice of implementation. Keeping spec #4/#12 unchanged
is the manual-write baseline alternative and avoids the facade, migration, and egress costs.

Two hybrid variants are worth distinguishing:

- **WebSocket writes plus HTTP status/reconciliation only:** avoids a second
  write ingress, while permitting outcome queries during socket problems. HTTP
  still needs authenticated owner routing; it cannot safely query a random box's
  empty cache as proof of non-placement.
- **WebSocket default plus signed HTTP writes:** useful where an upgrade is
  unavailable. Both share the same logical ID, dedupe table, key grant, and
  current owner admission. Neither races the other with independent IDs, and a
  transport switch would not authorize an Unknown resend. This has essentially
  Option 1's server/routing/authentication cost in addition to Option 2.

HTTP-first with WebSocket notifications is also possible, but retains those HTTP
costs and gives up the same-connection provenance benefit for manual writes.
Whether any hybrid's availability gain justifies that scope is open.

## Mobile-no-WASM question

**Question:** could mobile move directly to a server-writing DELEGATED client
without an on-device exchange engine or venue signer?

**Evidence-based answer: conditional architectural possibility, not current
support or an approved change.** Spec #4/#12 explicitly puts the full page engine
in the mobile WebView; #13 rejects replacing it with a facade. The proposed
server-writing client changes those points, rather than merely configuring
today's hosted mode. Mobile's existing Worker facade/bridge demonstrates a realm
boundary that could be replaced [M1], [M3], while the SDK's current fed path still
loads WASM and a signing host [S3]. Native server account methods/algos provide
an execution home, but its private construction is currently COSIGNED [S4], [S5].

If that delta were approved, a candidate mobile DELEGATED path would stop loading:

1. The hidden exchange-host WebView and its exchange Worker bundle [M2], [M3].
2. Per-account Rust **exchange-engine WASM families**, EngineHost fed-engine
   construction, and local order-planning/signing-host setup in that path [S3].
3. Venue-specific local signing/setup bundles and sign-request/sign-response
   bridge processing for that path; signing would live in the server account.

Under the refined DELEGATED join direction above, validation would sign on the
server using each connection's encrypted provided pair (STEER 13), so that
proposed DELEGATED client need not retain a venue signer merely for join. This
conclusion follows STEER 13 rather than STEER 11's earlier suggestion that the
client signs in both modes. COSIGNED would still need its client signer (STEER 12).
Encrypted handover/authentication crypto would remain even without a venue signer.

It could retain a TypeScript SCXF decoder, store/frame applier, hosted account
facade, subscriptions, and UI post-processors. The [existing FrameApplier][S10]
is TypeScript and imports wire definitions; a package called `safe-cex-wasm/wire`
is not by itself proof of runtime WASM instantiation. A no-engine build would
still need an import-graph/bundle check of that entire wire path. Charts,
Tealchart/Tealscript, account-key storage, and unrelated WebViews would not
automatically disappear. Option 1 still has request-authentication signing.

Open investigation: obtain the exact generated mobile SDK copies and deployed
exchange-host bundle, map all manual/settings/close/history/algo UI calls, verify
pure-TS frame decoding on Hermes, preserve derived UI store fields, and check
venue setup/nonce/address initialization is fully available server-side. The
missing generated copies mean an exact current download inventory is unverified;
the list above is the proposed removal boundary, not a measured bundle reduction.
No mobile source or generated/vendor tree was modified for this investigation.

## Key retention and lifecycle considerations

**Spec says:** handover has no expiry (#3), all tabs may close while algos continue
(#6), revocation wipes/stops (#7), restart loses keys and visibly pauses affected
algos while waiting for a fresh key (#8). **Possible change:** a 48-hour retention
limit after the last client disconnects would add an expiry to #3/#6. It is not
an already-approved exception. Changing restart from pause to terminal stop would
also change #8 and needs separate assessment.

One retention candidate would share a memory entry by **authenticated user +
account + credential incarnation**, referenced by that user's connections rather
than copying it per socket. Different accounts/keys would remain distinct. It
could start a monotonic 48-hour timer when the last participating authenticated
client disconnects, cancel it on an authenticated reattach, and restart it after
the next last disconnect. Whether all user connections or only connections
attached to that account count is open. Algo activity alone extending it would
be another policy choice, not an assumption.

A generation-bound timer could prevent an old disconnect timer from wiping a
replacement key after a new connection joins. At expiry, coordinated engine/algo
shutdown, queued-work revocation, final I/O fencing, and zeroization would avoid
“key dropped from map” being mistaken for loss of every cloned signer reference.
Whether expiry pauses like #8 or stops like #7 is unresolved. The 48-hour number,
not a credential-renewal workflow, is the proposal under discussion.

| Event                                        | Baseline                                                                                | Delta costs and open choices                                                                                                                                                                                                     |
| -------------------------------------------- | --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Last client disconnects                      | Account/algos continue (#6); no key expiry (#3).                                        | A 48-hour timer would bound unattended execution and memory exposure, but can interrupt long TWAP schedules. Multi-device connection accounting becomes part of custody.                                                         |
| Same-box reconnect                           | Reattach to running account (#6).                                                       | A fresh connection could attach to retained key custody after auth/lease checks; live binding would be renewed without reviving stale queued work.                                                                               |
| Process restart/crash                        | Key lost; visible waiting-for-key pause until re-handover, unless schedule lapsed (#8). | An alternative terminal stop with recorded reason would depart from #8. No in-memory retention scheme survives process loss. Unknown in-flight orders still need reconciliation.                                                 |
| Planned deploy roll                          | Current roller drains users, pins destination, then drains/restarts source [R5].        | Waiting for long-running jobs can delay a roll; forced movement can lose the only key copy. Carrying keys to a destination would add a sensitive transfer protocol, not follow from a router pin.                                |
| Graceful drain                               | Current admission blocks new algo starts while existing jobs finish [S11].              | A key-retention holder could keep a user resident longer. Idle-release/proof rules would need to account for retained custody, or explicitly retire/wipe it before releasing ownership.                                          |
| Box move/failover                            | Single-copy lease (#9); restart-style key loss is not transparent restore (#8).         | Destination could wait for an authenticated client to re-handover; offline jobs would pause under #8. Encrypted memory-to-memory transfer is a separate unapproved alternative with overlap, revocation, crash, and proof costs. |
| Disable/change/delete/remove or rejected key | Wipe and stop/record under #7/#10.                                                      | A timer would not delay those baseline actions. Shared references and queued work would be revoked together.                                                                                                                     |

The actual [roller, lines 95–112 and 155–179][R5] starts per-user drains and pins,
then drains/updates the box; it does not transfer venue keys. [The router proof
gate][R3] and [server proof contract][S11] establish release/admission, not proof
that all venue orders are canceled. For an abrupt crash, existing server journal
entries may describe work already issued, but a process cannot record a final
stop after it has died. A visible restart/key-loss reason would need an inference
from the new boot plus existing interruption/restore machinery, not a promise
that in-memory notices survived. Persisting non-secret job/outcome metadata is a
separate storage question; persisting the key is excluded by baseline #2.

## Outbox, idempotency, and resend considerations

**Spec says:** Unknown orders are never automatically re-sent (#14), and manual
writes use the page engine (#4). **Possible change:** a mobile outbox could retain
logical write intent across reconnects and expose progress. “Eventual delivery”
would then be bounded by user intent age, account ownership, explicit cancel,
key availability, and #14—not an unconditional promise of eventual execution.

### Candidate outbox and user states

| Candidate visible state | Meaning and possible transition                                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `queued`                | Intent recorded but no network handoff attempted. A live, same-owner, in-age entry could be sent after reconnect.                                       |
| `sent`                  | Handoff attempted; venue acceptance is not yet proven. A socket send return or server receipt would not mean accepted.                                  |
| `accepted`              | Recorded native/venue acceptance for the exact logical operation, with IDs and partial effects preserved. Not a claim of fill.                          |
| `refused`               | Definitive pre-dispatch refusal or venue rejection, preserving `notSent` versus `rejected`; expired/canceled unsent entries could show the reason here. |
| `Unknown`               | The operation may have reached the venue; query/reconcile rather than blindly resubmit.                                                                 |

A candidate persistence policy would retain entries across reconnect and app
background, and use a user-scoped encrypted local outbox across app kill. This
would persist intent/IDs/results, **not** the server's venue-key copy. A less
surprising alternative is to persist only sent/Unknown records across app kill
and require explicit re-submission of never-sent intents. Trade-off: continuity
versus an old order executing after reopening. Neither is selected here.

One concrete bound to discuss is **30 seconds max age for a never-sent manual
placement**, with no automatic replay while backgrounded and reevaluation on
foreground. Market orders might warrant a shorter bound or no offline queue;
cancels, settings, and algo-start intents might need different bounds. This is
not a proposed universal expiry for venue keys or already-accepted orders.
An entry could carry a server-time-derived acceptance bound plus local monotonic
elapsed time; after kill/reboot or uncertain clock calibration, refusing automatic
dispatch is safer than trusting a rolled-back wall clock. Server admission could
enforce the same bound. A deadline reset during reconnect would defeat it.

“Cancel queued” could atomically tombstone an unsent local intent before send.
If already received but provably undispatched, a shared server record could race
cancel and dispatch under one lock. Once handed off, cancel cannot promise the
order was never placed: it becomes a separate venue cancellation intent with its
own outcome. Pending/Unknown would not become `refused` just because the user
tapped Cancel. Tombstones and immutable IDs would prevent a second socket from
resurrecting the canceled intent. All waits, retries, persistence callbacks, and
flushes could carry the original authenticated owner/key incarnation and drop
stale work after logout/account change instead of relabeling it under a new user.

### Candidate shared idempotency and venue IDs

Spec #14 does not supply a general exactly-once delivery guarantee. A possible
extension would mint a random logical idempotency ID once at submit, preserve it
across transports/reconnects, and atomically claim
`(authenticated user, account/key incarnation, logical ID)` in the account's
shared server registry **across all of that user's sockets**, not per connection.
A canonical operation/payload digest would detect reuse of an ID with different
intent. A duplicate would return the original pending/result/refusal/Unknown
record, with no second venue dispatch. A batch could have deterministic child
IDs so accepted members are never resent with an uncertain suffix.

A minimum dedupe horizon could cover the selected outbox age plus admission/
execution/reconciliation uncertainty. Unresolved records and cancel tombstones
would not be evicted just because the never-sent age elapsed. Capacity pressure
could refuse new admissions instead of evicting unresolved evidence. These are
state-management costs, not a promise of unlimited retention.

Where venue format/uniqueness semantics permit, a domain-separated hash of the
logical ID plus server-derived account scope could produce a stable venue
client-order-id, with a documented encoding/length/collision policy per venue.
An ID alone does not prove venue duplicate rejection or lookup finality. Existing
caller-supplied client IDs and their SDK result contract would need a migration
choice; silently overwriting them would not preserve behavior.

An in-memory dedupe table gives at-most-one dispatch only within its surviving
custody/incarnation. Restart, eviction, or cross-box movement loses that table.
A new box's cache miss is not evidence that an old request never executed.
Durable **non-secret** outcome/intent metadata or transfer of such metadata could
improve recovery, but neither is currently selected; both have storage/privacy
costs and would not make the memory-only venue key durable. Without durable
evidence, missing history would conservatively remain Unknown. “Exactly once”
across arbitrary crashes and venues without reconciliation is not established.

The older no-nonce-machinery concern was about coordinating exchange signing
nonces across signers. Logical order idempotency is a different mechanism under
discussion; adopting it would not silently introduce a cross-signer nonce system.

### Proposed consideration against #14: reconcile before any resend

**Baseline #14 remains no automatic resend of an Unknown order.** Rust's proposed
consideration is: an outbox could resubmit only after venue reconciliation
**proves the original order was not placed**. Until such proof, a lookup failure,
missing capability, or inconclusive negative would retain Unknown with the literal
**“The outcome is unknown. Check Open Orders”** and would never trigger automatic
resend. This is a proposed narrowly defined change/interpretation of #14, not an
approved exception or a new generic repair entitlement.

| Per-action venue capability                                                   | Proposed treatment, if this delta were approved                                                                                                                                                                                             |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Stable client-order-id, authoritative lookup, and proven duplicate protection | Found order resolves the original record; no resend. Definitive non-placement could qualify for a controlled reattempt only after the original attempt can no longer later commit, within remaining intent age and current owner authority. |
| Lookup exists but `not found` is eventually consistent or incomplete          | Absence is not proof. Retain Unknown and query through existing reconciliation; no timed blind retry.                                                                                                                                       |
| Client-order-id exists but there is no authoritative lookup                   | Venue duplicate rejection is useful defense, but alone does not reveal an accepted/filled/canceled original. Retain Unknown without automatic resend.                                                                                       |
| No usable client-order-id, lookup fails, or evidence is ambiguous             | Retain Unknown and the exact guidance above. Never automatically resend.                                                                                                                                                                    |
| Cancel/amend/settings/close or multi-step/batch actions                       | Placement lookup alone is insufficient. Capability/evidence would be assessed per mutation/leg; accepted effects and IDs survive. No whole-batch replay or generic cancel-success inference.                                                |

No venue is certified into a category by this document. An implementation study
would enumerate actual per-venue/per-operation ID format, uniqueness lifetime,
lookup consistency, filled/canceled history, and finality. A negative lookup while
the original request remains in flight cannot justify a second send. Any proposed
proof-authorized reattempt could use a compare-and-swap transition on the same
logical record with separate attempt history, while ordinary duplicates still
return its recorded outcome. Reusing a fresh logical ID to bypass Unknown would
undermine both #14 and dedupe.

Existing engine journals preserve accepted/partial/Unknown effects; the
[repair-authority plan][S12] describes specific Phemex/Backpack repair scopes,
not blanket placement replay. A generic mobile outbox would not inherit those
entitlements. [The engine's immutable Unknown telemetry projection][S13] contains
no request, completion, or retry authority; logging an Unknown could not authorize
another order.

## Constraints compliance and observability

The options leave baseline #2/#3/#7/#10/#11/#15 intact except where an explicitly
identified delta is being discussed. No key persistence, logging, swap, trade-only
policy, IP restriction, credential-renewal flow, or new observability system is
proposed here. The 48-hour idea would change **no expiry** in #3; automatic
proof-based resubmission would be assessed against #14; stopping rather than
waiting after restart would change #8. Those remain open, not approved exceptions.

Process memory alone is not a complete key-lifecycle implementation: single
custodian leases, non-copying secret containers, signer task teardown, zeroization,
and deployment swap/core-dump controls would be reviewed under #2/#9. A server
key copy is already part of baseline DELEGATED, not a newly selected trust model.

Moving manual writes off the page also changes egress relative to #4: existing
direct/proxy/tunnel routes and IP-whitelisted keys cannot be assumed compatible
with server/proxy-fleet origin IPs. Preserving an account's existing route versus
exposing an unsupported-route refusal is an open compatibility question. It would
not be resolved by imposing new key IP/trade-only rules.

Under baseline #15, a candidate implementation could extend:

- **Engine events/journal:** keep exact native outcome, accepted prefix, command,
  request, and per-leg identity; connect logical ID to those existing IDs.
- **Requests Log and hosted verbose:** expose bounded, redacted delivery/duplicate/
  reconciliation metadata using existing events/frames. [requestsLog.ts,
  lines 10–43][S14] already excludes headers and logs off the execution path.
  No key-handover body or signature/authentication material would enter a log.
- **Server order audit and diagnostic journal:** associate logical ID, existing
  user/account/venue scope, outcome, and lifecycle reason with the existing audit
  allowlist [S15]. Baseline #11's server-signed-order audit would cover manual
  writes too if moved there; duplicates would be delivery observations, not
  fabricated second venue-order audit successes.
- **`order.outcome_unknown`:** retain the existing engine event and exact guidance
  from [unknown_outcome.rs, lines 1–30][S13]. Loss of a client acknowledgement is
  transport uncertainty and would not fabricate a second engine-owned Unknown
  event if the server already recorded a known native result. Telemetry failure
  would not affect admission, dedupe, outcome, or resend policy.

Recording a lifecycle reason could reuse existing interruption/restore notices
and audit machinery ([interruption.rs, lines 13–49][S16]); schema changes for new
reasons would be an explicit host-contract change, not assumed compatible today.

## Server work: possible scope, not an implementation plan

Estimates below are authored non-generated implementation lines plus separate
tests, not measurements or commitments. No code is included in this PR.

| Possible delta                                    | Likely modules                                                                                                                                      | Rough size and principal test burden                                                                                                                                                           |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Shared key custody and optional 48-hour lifecycle | New server key-custody module; accounts, native construction/signing, session release, algos, user drain/lease and interruption handling.           | 800–1,600 lines plus 700–1,300 tests; revocation, last-connection races, stale timer, rejection, shutdown, crash, roll/move, memory hygiene.                                                   |
| Shared key identity and pending join validation   | Server accounts/init/session, a pending-connection validator, remote-signer link/provider, native read builders, router/lease identity integration. | 700–1,400 lines plus per-venue and two-connection tests; canonical IDs, salt rotation, candidate wiping, current-token admission, no borrowed signer/state, native acceptance and roll storms. |
| Common write admission/dedupe/outcome query       | Server envelope, methods, session, accounts plus a shared logical-write registry; native engine journal mapping.                                    | 700–1,400 lines plus 900–1,700 tests; two users during every deferral, duplicate sockets, payload conflicts, partial batches, cancel/dispatch race, capacity, restart ambiguity.               |
| Option 1 ingress                                  | New signed-write HTTP/verifier module, HTTP routes, existing auth/lease admission; router index/UserRouter extension.                               | 600–1,100 lines plus 700–1,200 tests; canonical bytes, replay windows, skew/age, routing/drain/boot changes and lost responses. App request signing is additional client work.                 |
| Option 2 ingress                                  | Existing socket envelope/session, fresh key grants, bounded write/result handling; router custody reused.                                           | 250–600 lines plus 500–900 tests; handover provenance, attach permissions, reconnect, suspension, backpressure and owner change before dispatch.                                               |
| Hybrid                                            | Both ingress modules share one registry and binding model; status-only hybrid is smaller.                                                           | 200–500 extra integration lines plus cross-transport race tests, on top of the included paths. No second owner directory is assumed.                                                           |
| No-WASM mobile experiment                         | A native transport/outbox/store adapter, pure-TS frame path, replacement of exchange-host loading, SDK/UI callers and generated import graph.       | Separate mobile/NX work, not part of this doc PR. It explicitly changes #4/#12/#13 and cannot be estimated from the missing generated tree alone.                                              |

These scopes overlap; adding all rows is not a reliable total. Reusing #3391's
owner binding, once available, does not remove live lease/incarnation tests.
Any later client/server envelope or SDK facade change would need explicit host
coordination (**NEEDS-NX for implementation**, not a code change in this document).

## Risks/open questions for Sam

1. Is reopening #4/#12/#13 for mobile manual forwarding desirable, including the
   previously rejected facade and account-route/IP-whitelist compatibility cost?
2. Which transport warrants a prototype: signed HTTP, same-connection WebSocket,
   status-only hybrid, or dual-write-ingress hybrid? How much operational benefit would HTTP fallback add beyond the decided
   per-connection JWT and venue validation? The DELEGATED provided-pair handover
   would still occur on every connection.
3. Should #3/#6 gain a retention bound, and is 48 hours the right number? Which
   connections count toward “last client,” and would expiry pause or stop algos?
   Keep #8's waiting-for-key behavior, or explicitly change restart semantics?
4. For a roll/move, is pause-and-rehandover acceptable, or is encrypted volatile
   key transfer worth specifying separately? How would retained custody interact
   with existing idle-release and per-account leases under #9?
5. Should never-sent intents survive app kill, or only sent/Unknown evidence?
   Are 30-second placement age, foreground-only flush, and atomic queued cancel
   suitable candidates? What bounds apply to market, cancel, settings, and algo
   intents separately?
6. What dedupe/outcome metadata durability is acceptable without persisting keys?
   How long should unresolved records survive, and what happens on custody loss?
7. Does the proposed proof-of-non-placement policy warrant any explicit change
   to #14? Which actual venue/action capabilities can establish final absence,
   rather than a transient `not found`? Keeping #14 unchanged remains an option.
8. Can a complete generated mobile tree and exact hosted bundle validate the
   no-WASM import graph, UI compatibility, and full operation coverage before
   choosing a mobile-only migration? No implementation direction is selected here.

## Evidence references

The links below identify source snapshots and line ranges used in the analysis.
The mobile and router repositories may require contributor access.

[S1]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/docs/plans/TS_RUNTIME_DELETION_INVENTORY.md#L12-L15
[S2]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/docs/plans/RUST_FRAME_FED_READ_COVER.md#L10-L13
[S3]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/exchanges/rust/rust.exchange.ts#L1183-L1202
[S4]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L1125-L1153
[S5]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/remote-signer/src/lib.rs#L71-L141
[S6]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/http.rs#L792-L825
[S7]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/auth.rs#L89-L104
[S8]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L380-L393
[S9]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/router_lease.rs#L137-L193
[S10]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/store/frameApplier.ts#L1-L43
[S11]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/DESIGN.md#L176-L237
[S12]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/docs/plans/RUST_REPAIR_MUTATION_AUTHORITY.md#L14-L24
[S13]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/engine/src/unknown_outcome.rs#L1-L30
[S14]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/transport/requestsLog.ts#L10-L43
[S15]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/audit.rs#L1-L39
[S16]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/interruption.rs#L13-L49
[M1]: https://github.com/Tealstreet/tealstreet-mobile/blob/8377a4f3f1e048df14ad76be93bf2837366bcd60/src/web/utils/shim/worker/workerFactories.ts#L5-L175
[M2]: https://github.com/Tealstreet/tealstreet-mobile/blob/8377a4f3f1e048df14ad76be93bf2837366bcd60/src/mobile/components/ExchangeHostWebView.tsx#L30-L141
[M3]: https://github.com/Tealstreet/tealstreet-mobile/blob/8377a4f3f1e048df14ad76be93bf2837366bcd60/src/mobile/services/exchangeHostBridge.ts#L10-L103
[M4]: https://github.com/Tealstreet/tealstreet-mobile/blob/8377a4f3f1e048df14ad76be93bf2837366bcd60/babel.config.js#L6-L59
[M5]: https://github.com/Tealstreet/tealstreet-mobile/blob/8377a4f3f1e048df14ad76be93bf2837366bcd60/patch-config.json#L490-L529
[P3391]: https://github.com/Tealstreet/tealstreet-sdk/pull/3391
[O1]: https://github.com/Tealstreet/tealstreet-sdk/blob/ec49cdd8159600a23baf60afb344cad5b50fc119/sdk-rs/hosts/server/src/twap_owner.rs#L5-L45
[O2]: https://github.com/Tealstreet/tealstreet-sdk/blob/ec49cdd8159600a23baf60afb344cad5b50fc119/sdk-rs/hosts/server/src/accounts.rs#L1176-L1177
[R1]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/workers/exchange-router/src/index.ts#L18-L73
[R2]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/workers/exchange-router/src/userRouter.ts#L250-L408
[R3]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/workers/exchange-router/src/userRouter.ts#L195-L215
[R4]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/workers/exchange-router/src/userRouter.ts#L52-L78
[R5]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/infra/tofu/jumpbox/exchange-roll.sh#L95-L179
[WS]: https://www.rfc-editor.org/rfc/rfc6455.html#section-1.2
[I1]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L449-L473
[I2]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L911-L943
[I3]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L708-L744
[I4]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L1013-L1038
[I5]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L1375-L1410
[I6]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/hyperliquid/src/lib.rs#L26-L50
[I7]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/lighter/src/lib.rs#L40-L57
[I8]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/paradex/src/lib.rs#L17-L45
[I9]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L546-L556
[A3]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/session.rs#L414-L497
[A4]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/session.rs#L373-L404
[A5]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L1793-L1839
[J1]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L268-L297
[J2]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/remote-signer/src/link.rs#L73-L183
[J3]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/remote-signer/src/link.rs#L212-L232
[V0]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/types.ts#L117-L122
[V1]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/exchanges/base.ts#L1008-L1010
[V2]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/exchanges/rust/rust.exchange.ts#L2511-L2575
[V3]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/exchanges/rust/rust.exchange.ts#L388-L389
[V4]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/exchanges/rust/rust.exchange.ts#L1882-L1887
[V31]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/exchanges/rust/construction-rules.json#L1-L50
[V5]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts#L144-L167
[V6]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts#L82-L96
[V7]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts#L314-L339
[V8]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts#L929-L957
[V9]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts#L134-L161
[V10]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts#L335-L347
[V11]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts#L426-L448
[V12]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts#L171-L190
[V13]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts#L190-L200
[V14]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts#L241-L258
[V15]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts#L159-L177
[V16]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts#L1595-L1674
[V17]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts#L109-L130
[V18]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts#L913-L928
[V19]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts#L103-L124
[V20]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts#L141-L160
[V21]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts#L121-L130
[V22]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts#L154-L163
[V23]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts#L165-L174
[V24]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts#L184-L203
[V25]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts#L353-L389
[V26]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts#L288-L324
[V27]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts#L436-L463
[V28]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts#L864-L876
[V29]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts#L904-L961
[V30]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts#L200-L220
[A1]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/workers/exchange-router/README.md#L8-L41
[A2]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/workers/exchange-router/src/token.ts#L41-L71
[V2-timeout]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/exchanges/rust/rust.exchange.ts#L515-L515
[V5-futures]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts#L325-L350
[V5-inverse]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts#L208-L231
[V6-options]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts#L77-L83
[V8-inverse]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts#L386-L405
[V8-spot]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts#L235-L254
[V8-demo]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bybit/bybitdemo.exchange.ts#L1-L17
[V12-spot-balance]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bingxspot/bingxspot.exchange.ts#L148-L168
[V12-spot-paths]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/bingxspot/bingxspot.exchange.ts#L22-L33
[V14-eu]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/okx/okxeu.exchange.ts#L38-L47
[V15-spot]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts#L109-L125
[V16-spot]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts#L661-L702
[V28-spot]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts#L116-L127
[VL1]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/lighter/src/account.rs#L1036-L1069
[VL2]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/lighter/src/account.rs#L879-L943
[VL3]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/lighter/src/sign.rs#L801-L811
[VL4]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/lighter/src/sign.rs#L935-L971
[VL5]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/lighter/src/sign.rs#L973-L994
[VL6]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/venues/lighter/tests/auth_recipe.rs#L183-L205
[S8-init]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L946-L966
[S8-placement]: https://github.com/Tealstreet/tealstreet-sdk/blob/05241cd0f41b05c382fceed0e0347d9b81729a0c/sdk-rs/hosts/server/src/accounts.rs#L1125-L1195
[R2-upgrade]: https://github.com/Tealstreet/tealstreet-next/blob/87aa7a97cc8c2f69f91e95d81297314f5eb1a4bc/workers/exchange-router/src/userRouter.ts#L469-L478
