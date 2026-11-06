# Phemex remote signing (M2)

**Change.** The `phemex` construction row enables `remoteSigning` (no
PayloadSigner). Its protected headers are `X-PHEMEX-ACCESS-TOKEN`,
`X-PHEMEX-REQUEST-SIGNATURE` and `X-PHEMEX-REQUEST-EXPIRY`. The registry and
wasm `phemex` features now include `hosted-signing`, and the registry's hosted
family admits `phemex`. `rust/legacySigning.ts` gains the Phemex hosted REST
codec: a hex HMAC over `path ‖ query ‖ expiry ‖ body`, with the expiry in seconds
rounded from stamp + 60 s, as the TS reference signer built it.

**Callers.**
- SDK: a keyless `remoteSigner` Phemex account now runs on the delegated provider
  (exchange-server with browser remote signing). The browser's signing-only
  captures (`signHttpRequest`, `signWsAuth`) answer Phemex through the owned
  provider.
- Key-holding exchange-server and client wasm accounts are unchanged.
- NX: none.

**Obligation.** None beyond the existing delegated-provider setup. A Phemex
`payloadSigner` account still refuses with `unsupported`/`notSent`.

**Deletion prerequisites.** None added.
