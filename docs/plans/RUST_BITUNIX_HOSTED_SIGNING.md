# Bitunix hosted signing on Rust (P7)

Owner: claude-4yjlw0. Assignment: claude-ddi2il (after beta R1). GO: claude-8x5tm1 (registry, wasm host types, TS signer provider). Review: claude-dazzy3. One PR: plan, originals and source.

## Why

R1 (#2255) refuses delegated signing (`remoteSigner` and `payloadSigner`) on IN venues whose `construction-rules.json` row lacks `remoteSigning.enabled`. Bitunix is one of them. Hosted Bitunix users (keyless account, browser signs) stay on frozen preview until Rust signs hosted Bitunix requests. This is the P7 "hosted halves" row of `RUST_SIGNER_NON_HMAC_MIGRATIONS.md` for Bitunix, whose half 2 is already possible on today's wire (`bitunix-double-sha256`).

## Today

- The engine signs Bitunix at dispatch (`venues/bitunix/src/sign/dispatch.rs`, #2190).
  - One recipe for REST (`Http`) and the private socket login (`WsLogin`).
  - Steps: `Sha256[Bytes(nonce‖stamp), Credential(key), Bytes(query‖body)]`, then `Sign Sha256SecretSuffix [Result{0, Hex}]`, then `Present(key)`.
- The TS venue signed hosted accounts through `remoteSigner.sign({type: 'ws-auth', exchangeName: 'bitunix', wsAuth: {toSign, outputFormat: 'bitunix-double-sha256', extraData}})` (`bitunix.ws-private.ts:504-516`).
  - The browser computes `hex(sha256(hex(sha256(toSign)) ‖ secret))` (`base.ts:2362-2368`) and ignores `extraData`.
  - That primitive is exactly the engine recipe's inner and outer steps, for any `toSign`, REST preimages included.
- Neither delegated path admits the recipe today:
  - The TS remote signer provider accepts only `present` and `sign hmacSha256` (`remoteSignerProvider.ts:269-292`).
  - The registry's hosted codec builds only HMAC work (`registry/src/hosted_signing.rs`).

## Change

1. **Construction row.** Bitunix gains `"remoteSigning": {"enabled": true, "payloadSupported": false, "protectedHeaders": ["api-key", "sign", "nonce", "timestamp"]}`.
   - The hex signature is the default, and Bitunix has one alias, so no `wire` is needed.
   - `build_support.rs` already requires `dispatchSigning` and `protectedHeaders` for an enabled row.
   - No new key, so no `build_support` schema change.
2. **Half 2: keyless engine account, the browser signs (`remoteSignerProvider.ts`).**
   - The provider admits exactly one fused shape: a `sha256` operation whose parts are bytes, or the signing identity's own credential, referenced by one `sign sha256SecretSuffix` whose only part is `Result{that operation, hex}`, plus `present` of the same credential.
   - It sends one `remoteSigner.sign` per batch: `type: 'ws-auth'`, `exchangeName: 'bitunix'`, `toSign` = the UTF-8 inner preimage, `outputFormat: 'bitunix-double-sha256'`.
   - It returns values in operation order: the inner SHA-256, computed locally; the 32 decoded outer bytes; and the key bytes from local custody.
   - A credential part from another identity, any other shape, or a payloadSigner is a typed refusal. `payloadSupported` stays false: payloadSigner has no scheme for this.
   - The legacy WS `extraData {timestamp, nonce}` is not sent, since the browser ignores it and HTTP and the login share one part shape.
   - The signer's advertised algorithms become per venue (`remoteSignerAlgorithms(name)`), so `sha256SecretSuffix` is advertised only for Bitunix, never for the HMAC venues.
3. **Half 1: keyed browser engine, the server worker asks it to sign.**
   - `registry/src/hosted_signing.rs` gains a Bitunix family (behind `cfg(feature = "bitunix")`). Its work is `[Present credential, Sha256{Bytes(toSign)}, Sign Sha256SecretSuffix [Result{1, Hex}]]`, with output format `BitunixDoubleSha256` and the closed-schema parser.
   - `finish` stops assuming the MAC is `values[1]`.
   - TS: `HostedSigningInput` (`hosts/wasm/js/host.d.mts`) and `signHostedWsAuth` (`legacySigning.ts`) accept `bitunix-double-sha256`.
   - A standalone `bitunix.signing.ts` gives `signHostedHttp` the Bitunix request preimage, as `prepareBitgetRequest` does.
4. **Unchanged:** the engine, crypto, the Bitunix venue crate and its recipes, registry keyless construction (identities plus a provider), signing and credential inventories, and the wasm features (`bitunix` already enables `dispatch-signing`; `sign-meta` is Kraken and Paradex only).

## Behaviour kept and pinned

- **Wire bytes.**
  - Hosted REST headers and the hosted login frame are byte-identical to a local-key account for the same key, nonce and stamp. The API key is presented from local custody, never from the browser.
  - The remote `toSign` for the login equals TS's `nonce‖ts‖apiKey`. For REST it is `nonce‖ts‖apiKey‖query‖body`, the TS REST preimage.
- **Freshness.** The 5 s signing freshness and the stamp floor are unchanged. A slow browser answer re-signs and never sends a stale stamp.
- **Refusal latching.** A remote refusal or timeout on the login latches the role, as for Bitget, with no tight loop.
- **Primary only.** Secondary (read-key) remote signing stays a typed refusal, as for every half-2 family.

## Originals (RED first)

1. **Half 2:** a keyless Bitunix account with a fake `remoteSigner` signs REST and the socket login through `bitunix-double-sha256`. The headers and login frame equal the local-key golden, and `toSign` equals the TS preimage.
2. **Half 2 refusals:**
   - a credential part not owned by the signing identity is refused;
   - payloadSigner is refused;
   - `sha256SecretSuffix` is not advertised for Bybit, Binance, OKX or Bitget.
3. **Half 1:** `prepareHostedSigning` for Bitunix HTTP and WS returns the TS golden signature, from the Node-crypto vector in `base.ws-auth-signing.test.ts`.
4. **R1 eligibility:** a keyless Bitunix `remoteSigner` account constructs on Rust, and its payloadSigner is still refused.

Mirrors: `registry/tests/bitget_hosted_signing.rs`, `okx_hosted_signing.rs`, `rust.bitget-hosted-provider.wasm.test.ts`, `rust.remote-signer-provider.test.ts`, `rust.delegated-signer-setup.test.ts`.

## Decisions for review

- **D1: no `extraData` on the hosted login.** The browser contract ignores it, and keeping it would need a separate login codec.
- **D2: per-venue signer algorithms.** Global would advertise a Bitunix-only algorithm to every HMAC venue.
