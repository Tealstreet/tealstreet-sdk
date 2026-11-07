# BloFin hosted and remote signing

Change: BloFin joins the dispatch-signing families with hosted
`signHttpRequest`/`signWsAuth` and keyless `remoteSigner` admission
(`construction-rules.json` row, `signatureEncoding: "hex-base64"`).

Callers affected:

- **SDK / web**: none. `createExchange('blofin', ...)` consumes credentials
  through trusted signer setup, as for Bitget and OKX.
- **Server (exchange-server)**: a hosted BloFin account may now run keyless with
  a `remoteSigner`; the browser answers `ws-auth` requests with
  `outputFormat: 'hex-base64'`. A private `payloadSigner` is refused.
- **Native callers of `build_with_signer`**: BloFin key, secret, passphrase and
  read credentials belong in signer setup; engine options carrying them are
  refused. `build` still consumes raw options through trusted setup.

Behaviour change: a private BloFin account with a missing or empty Primary
passphrase, or an explicit empty Secondary passphrase, is refused at
construction instead of starting and failing at the first signed request.

Deletion prerequisites: none added or cleared.
