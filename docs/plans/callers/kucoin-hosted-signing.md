# KuCoin hosted signing — hold for Sam

This production signing change must remain unmerged until Sam approves it.
It starts after #3170 (792d8e51540). KuCoin Spot is outside this activation.

## Diagnosis and admission

KuCoin already belongs to the Rust beta roster, registry, metadata and WASM
family loaders. `assertBetaAccount` refused delegated accounts because the
KuCoin construction declaration had no enabled `remoteSigning` policy. The
user-facing error is unchanged. One construction-table entry now admits
RemoteSigner; private PayloadSigner remains unsupported. Tests retain the
refusal for undeclared KuCoin Spot, Kraken Spot and Crypto.com accounts.

Admission alone was insufficient: KuCoin signs the request and passphrase
separately, with an optional third broker MAC. Generic hosted HMAC capture
neither resolved this graph nor admitted KuCoin. The KuCoin WASM feature now
links its device batch codec.

## Device protocol

`ws-auth` with `outputFormat: kucoin-batch` carries a closed JSON descriptor:
`{slot, payload, brokerPrefix?}`. `slot` selects the device's primary or secondary
identity. `payload` is the exact native request preimage; `brokerPrefix` is the
public timestamp plus partner label. The device resolves its own API signing,
passphrase and optional broker refs. It returns a JSON string containing its
public credential and two or three canonical base64 MACs. The origin checks
that credential against its selected identity, validates every MAC, and returns
the whole batch atomically. Raw passphrase signing formats are refused.

The origin can operate without an API secret, plaintext passphrase or broker
key. Those fields stay in the device's local signer setup. Opaque refs are not
sent to the device. Secondary reads bind the secondary device identity; the
broker ref aliases the primary broker key as in local setup. Missing device
material, foreign refs, malformed/partial batches, callback errors, wrong-account
answers and cancellation refuse without fallback or partial publication.
Errors discard callback text; fixtures check that logs, errors and the remote
wire contain no signing material.

## Parity evidence

The regressions were written and run red before implementation:

- `rust.kucoin-hosted-signing.test.ts`: missing admission and unsupported batch
  setup failed. The five tests now cover positive admission, undeclared-venue
  refusal, unsupported PayloadSigner, exact remote payload/MAC bytes, malformed
  batches/answers, redacted errors and canceled late completion.
- `registry/tests/kucoin_hosted_signing.rs`: the batch format was unknown and
  missing-passphrase capture returned the wrong refusal. Device custody,
  missing material, closed descriptor validation and capture clock fencing now
  pass. A changed clock revision retires the old work before a fresh capture;
  malformed completion also retires its token.
- The existing KuCoin native fixture provider now compares every actual signing
  graph with the hosted capture graph before comparing all resulting MACs. This
  runs under the existing fixed clocks and seeded entropy, including the
  `clock_recovery` fixtures, failed/Unknown cancellation and amend paths, and
  #3170's protective rollback/guard fixtures. The helper preserves local
  immediate completion, so the control does not alter action scheduling.
- `rust.kucoin-hosted-provider.wasm.test.ts` runs actual WASM local/device and
  keyless remote/origin hosts. Its 27 endpoint fixtures run at two fixed times;
  payloads, signed configs, request MACs, encrypted passphrases and broker MACs
  are identical. Additional cases cover secondary custody and refusal/error
  redaction. Network access is forbidden by the fixture transport.

The endpoint matrix covers catalogue/tickers, identity/account/balance,
positions, active and historical ordinary/stop orders, fills, ledger and funding
history, margin/position mode and leverage reads/writes, ordinary and protective
placement, individual and bulk cancellation, depth and public trades. Amend is
cancel, terminal-proof GET, then replacement POST; protective rollback uses the
same placement/cancel recipes. WebSocket authentication uses the signed
`POST /api/v1/bullet-private`, followed by a token-authenticated socket URL;
there is no separate KuCoin socket-login MAC. Clock probes remain unsigned.
KuCoin has no request nonce or signed receive-window field; the fixtures fix
protocol timestamps, request IDs/entropy and transport receive-window defaults.

These are deterministic signing proofs, not live exchange orders. CI and
maintainer review remain authoritative; Sam's approval is required before merge.
