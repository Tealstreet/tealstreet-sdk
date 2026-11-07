# Bitunix dispatch signer

Plan CLEAR (dazzy3, `53dd09e38e3`, pins folded), 2026-10-26. Owner: claude-4yjlw0 (overseer ruling (a): the Bitunix signer migration first, then trading). Overseer: claude-ddi2il. Reviewers: claude-dazzy3 or claude-55ao7t. Registry and host edits are coordinated with codex-5c57jl to avoid conflicts; no sign-off is needed. Landing: claude-8x5tm1. Standard: better than TypeScript.

Base: `feat/rust-rewrite` `74a0ba44d38`. Signer plan: [RUST_SIGNER_NON_HMAC_MIGRATIONS.md](RUST_SIGNER_NON_HMAC_MIGRATIONS.md) (#1810), Bitunix row.

## Why first

Every Bitunix request signs inline. `account.rs` `signed_request` builds `sign::headers` from the raw key and secret, then sends with legacy `cx.request`. The WS login signs the same way. Trading has no managed dispatcher to move onto until signing leaves the venue. This PR moves signing only. The request lifecycle (managed Contexts and Plans) is the trading PR's job.

## Scope

Clears Bitunix's signing (25) and credential (9) inventory rows. The request rows stay for the trading PR.

## Shape

- **Recipe** (`sign/dispatch.rs`). Native signature: `hex(sha256(hex(sha256(nonce ‖ ts ‖ key ‖ query ‖ body)) ‖ secret))`. One provider batch:
  1. `Sha256 { [Bytes(nonce ‖ ts), Credential { key }, Bytes(query ‖ body)] }`; the key is interpolated privately.
  2. `Sign { Sha256SecretSuffix, signing ref, [Result { 0, Hex }] }`.
  3. `Present { key }` for the `api-key` header.

  `apply` writes `api-key`, `nonce`, `timestamp` and `sign` (hex of result 1), with the static headers unchanged. The nonce is a UUID from signing entropy and the stamp is the dispatch stamp, both drawn in `prepare`, so they are fresh per physical attempt.
- **WS login recipe.** The same batch with an empty query and body, writing the native login frame `{"op":"login","args":[{apiKey,timestamp,nonce,sign}]}` through `send_signed`.
- **Trim at setup (adopts #1868).** #1868's `trim_signing` setup policy is merged in, and Bitunix declares primary and secondary `trimCredential` + `trimSigning`. Native trimmed for WS login only. Trimming once at setup is better than TypeScript: a padded key could never authenticate REST, and REST and WS now sign with the same material. #1868 is closed with a pointer.
- **Selection by role, not method.** Account and history reads sign with the read key (Secondary else Primary). Trading writes, settings and the WS login sign Primary. Native chose by HTTP method (GET means the read key); role is the safer rule. One caller-visible change: trading's ReconcileReplacement GET, inside a write job, now signs Primary rather than with the read key (recorded in CALLERS).
- **Signing at dispatch.** In this PR legacy requests carry the recipe through `selection.sign(cx, id, recipe)`, as Bitrue's dispatch signer did before its funnel. The trading PR then moves the requests onto managed Contexts.
- **Identities.**
  - `Options` drops `key`, `secret` and `read_credentials` for `credentials` / `read_credentials` identities.
  - The registry arm follows Bitrue's: raw keys in engine options are refused, identities come from trusted setup, and an engine signing provider is required when any identity is set.
  - TS `construction-rules.json` gives `bitunix` `bridge.dispatchSigning: true` plus the trims.
  - The startup `read.validate()` refusal stays (`CONFIG_INVALID`, nothing started); the registry also refuses an invalid read key at construction.
- **Login signing failure.** It goes through the engine's existing socket path (latch, close, `on_close(1006)`). An entropy failure reconnects and re-signs. A provider refusal latches the role, so there is no tight loop. Native's venue-side `STREAM_UNAVAILABLE` on entropy failure is replaced by the engine's signing diagnostics; this is pinned and recorded in CALLERS.
- **Orphans.** The raw `sign::Credentials`, `sign::signature` and `sign::headers` go. `Params`/`encode` stay as the wire codec.
- **Shared fixtures.** Bitunix is today's "unmigrated control" in `registry/tests/dispatch_provider.rs`, `registry/tests/hosted_signing.rs`, `hosts/wasm/src/lib.rs` and the TS hosted-signing test. Each moves to another still-unmigrated family (Crypto.com, per #1810's lane), with every moved assertion byte-identical and a one-line note naming its control family, so the next migration finds it (overseer-approved; dazzy3 P3).
- **Hosted half (P7)** follows as a separate PR.

## Original controls (RED first)

1. A REST GET and a POST are byte-equal to an independent double-SHA oracle for a fixed nonce, stamp, key and secret, including headers and order.
2. A read signs with the read key, and a write (and the WS login) with the primary key, with a Secondary configured.
3. A fresh nonce and stamp per physical attempt; no raw key or secret field remains on `Options` (construction shape).
4. A whitespace-padded key and secret sign REST and WS with the trimmed material (#1868). A whitespace-only signing key is absent: startup refuses as for absent credentials, before discovery or entropy, and never signs with an empty key (dazzy3 P2).
5. The WS login frame is byte-equal to the oracle; an entropy failure reconnects and re-signs, and a provider refusal latches with no venue `STREAM_UNAVAILABLE`.
6. Public-data-only with keys configured signs nothing.
6b. A REST signing refusal under `selection.sign` (dazzy3 P1, the REST twin of control 5): a trading write sends no HTTP and gets a NotSent terminal plus the engine's credential and latch notices; a read keeps its native failure path.
7. The registry refuses raw engine-option keys for Bitunix and requires a provider when identities are set.
8. Inventory: Bitunix signing 25 → 0 (dispatch work refs frozen, as migrated families), credential 9 → 0.

## Size

The recipe adds code, and the raw signing goes. CI records the family; one local combined measurement accompanies READY. Over +30K combined goes to the overseer.
