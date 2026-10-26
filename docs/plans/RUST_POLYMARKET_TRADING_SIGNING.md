# Polymarket trading and signing on managed authority

Plan, 2026-10-26. Owner: claude-fe7lgm. Reviewers: claude-dazzy3 or
claude-55ao7t; m26ps4 for the identity and setup fit. Base: `feat/rust-rewrite`
`3db1219f60e`. Sockets are out of scope (6 sites, a later slice).

## Current shape

Polymarket computes every signature eagerly in the venue, from raw option
material:

- **L2 HMAC** headers (`POLY_ADDRESS`, `POLY_SIGNATURE`, `POLY_TIMESTAMP`,
  `POLY_API_KEY`, `POLY_PASSPHRASE`) for private REST calls.
  - HMAC-SHA256 over `"{seconds}{METHOD}{path}{body}"`.
  - The key is the URL-safe base64 `api_secret`.
- **EIP-712 V2 order** signing with the wallet `key`, via `sign_digest`, with v
  as 27/28. Signature type 3 wraps it in ERC-7739.
- **L1 ClobAuth** signing with the wallet key, used to derive L2 credentials
  from `/auth/derive-api-key` when none are configured.

All private HTTP goes through `account.rs:283` `signed_request` (`cx.request`).
Trading terminals are raw `cx.done`/`cx.fail` (`trading.rs:330-632`). No
Identities or LocalSigner are built for Polymarket today.

## Engine prerequisites (owner: fe7, one small engine PR)

1. **`KeyEncoding::Base64Url`** for HMAC key material (URL-safe alphabet, padding
   optional), so the configured `api_secret` installs as an HMAC identity
   without venue-side decoding.
2. **An EVM recovery-id encoding for result splices.**
   - A POST `/order` HMAC covers a body that contains the order signature. So,
     as ApexOmni does, one work splices the Secp256k1Digest result into the HMAC
     payload (`PayloadPart::Result`).
   - Polymarket needs v as 27/28 (`0x1b`/`0x1c`); today the splice can only
     write 0/1.
   - Add a `Result` encoding that renders `r || s || (v + 27)` as hex. The
     engine owns it; there is no venue-side byte fixing.
3. **Derived L2 credentials (P6b)** need a runtime provider import of a secret
   the venue receives from `/auth/derive-api-key`. This needs an owner decision
   before any derived-credential path moves. Configured credentials do not need
   it.

## Slices

**Slice A, this lane's first PR (after prerequisites 1-2):** configured L2
credentials plus the wallet key.

- Registry trusted setup builds:
  - an HMAC Primary identity from `api_secret` (Base64Url), with Present refs
    for `api_key` and `passphrase`;
  - a Secp256k1Digest `L1Wallet` identity from `key` (the existing
    `ExtraKey::L1Wallet`).
  - It stops copying raw strings into `PolymarketOptions`.
- `sign/dispatch.rs` recipes, modelled on Bitrue and BloFin (HMAC) and
  ApexOmni (result splice):
  - `L2` (an HMAC header recipe with a seconds stamp);
  - `Order` (EIP-712 digest Sign, then the HMAC over the body with the spliced
    signature, in one work, with apply rendering the native body);
  - `PublicAddress` for `POLY_ADDRESS` from the installed wallet ref.
  - The ERC-7739 wrapper stays venue codec, built around the provider result.
- Trading on managed authority (BloFin W1 shape):
  - `begin_preflight(WriteAfterReads, read Primary, write Primary)`;
  - one job at a time per scope, via FIFO `park_preflight` (native rejects
    conflicts, which this keeps as a refusal);
  - serial Plans for place batches (strict order), amend (cancel, then verify
    up to 3, then place), and cancel/cancel-all;
  - `classify_write` with a cited CodeTable; network failure is Unknown, as
    native, with no write retries.
- Account reads (orders, balance) become Background SelectedRead with the L2
  recipe. Public `/time` and positions stay public.
- Derived credentials are refused at startup with a typed config error until
  P6b lands; the configured path is unaffected. Record this in CALLERS as a
  capability gap that has to be ruled on before landing, since it removes the
  derive fallback.

**Slice B (after P6b):** the L1 derive path on a provider import.

**Slice C:** sockets.

## Original controls (Slice A, RED first)

- An exact L2 header set for GET `/data/orders` and DELETE `/order`, from the
  provider, matching the `sign.rs` official vectors and `tests/account.rs:217`.
- POST `/order` body plus HMAC byte-identical to `tests/trading.rs:161`
  (independent alloy EIP-712 hash), for signature types 0-3 and negRisk. The
  splice carries v as 27/28.
- No raw key, secret or passphrase in `Options` (construction shape), and the
  `sign.rs` raw-signing helpers removed from non-test code.
- Place, amend and cancel sequences through Preflight and serial Plans, with
  native outcomes (Unknown on network loss, no resend) and FIFO refusal parity.

## Gates

- Polymarket scoped tests, strict clippy, fmt and bans.
- Inventories: request 7 -> 0 and signing 24 -> 0 (Polymarket joins the
  migrated sign set).
- The prerequisite engine PR: paired Bybit/Bitget plus a combined pair.
- Slice A: the CI family record plus one combined pair if the engine
  prerequisites land in the same window.
