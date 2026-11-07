# KuCoin Spot hosted signing

KuCoin Spot uses the shared Rust `sdk-rs/venues/kucoin` crate with its `spot`
feature. The construction rule now admits RemoteSigner using the same
`kucoin-batch` protocol as Futures. The standalone Spot WASM family includes
its own hosted-signing codec; it does not depend on the combined build or on
loading the Futures family.

## Authentication and custody

Every native signed request uses the shared dispatch recipe. A batch presents
the selected public API key, signs the exact UTF-8 bytes of timestamp + method +
path/query + serialized body with HMAC-SHA256, and separately signs the
setup-owned passphrase. Optional broker attribution adds a third HMAC over
timestamp + partner + public API key. All MACs use padded base64.

The RemoteSigner receives only `{slot, payload, brokerPrefix?}` in a
`ws-auth` request with `outputFormat: kucoin-batch`. The device resolves its own
primary or secondary identity, passphrase and optional broker key. The origin
retains opaque refs and public routing metadata, and checks the returned public
key and the whole MAC batch before publication. It never sends passphrase or
broker-key material to the device. Errors and canceled requests do not publish
partial signatures or fall back to local credentials.

WebSocket authentication is the signed `POST /api/v1/bullet-private` request,
followed by the returned token in the socket URL. There is no separate login MAC.
Public feed tokens and clock probes remain unsigned in the native lifecycle.
Clock-skew recovery re-signs an eligible read using the selected identity and
corrected protocol time. Writes are never replayed after an ambiguous result.

## Delegated modes

Remote custody supports both primary trading and secondary read API keys. The
secondary slot uses its own device-owned passphrase and the configured shared
broker key. KuCoin Spot does not expose an agent/API-wallet signature mode in
this integration. `PayloadSigner` stays disabled: one canonical request HMAC
cannot supply the additional setup-owned passphrase and optional broker MACs.
A configured unsupported private mode fails with `unsupported` / `notSent`
before I/O. Explicit public-only accounts ignore all signer configuration and
install no signing authority; an unused PayloadSigner cannot reject construction.

## Caller impact and validation

No public TypeScript type or remote wire format changes. Callers use the existing
RemoteSigner and `kucoin-batch` request/response contract. The only new public
construction policy is enabled remote signing for `kucoinspot`, with
`payloadSupported: false`, base64 signatures and the same protected headers as
Futures. NX needs no separate activation list change: the derived refusal list
from NX #9032 follows the construction rule when this SDK version is consumed.

Shared TS regressions run for Futures and Spot, including admission, refusal,
exact local/remote MAC bytes, wrong-account/malformed answers, cancellation and
secret redaction. Actual WASM tests compare local/device and keyless remote/origin
signing across the Spot endpoint inventory at two fixed timestamps, including
private socket token auth and secondary custody. The native fixture provider
compares each actual Spot request graph and MAC with the Spot hosted capture,
covering startup, pagination, clock recovery and trading/failure lifecycles.
The construction-feature regression checks the isolated family, and native
registry tests exercise that family without enabling Futures.

These are deterministic fixtures without live orders. CI and maintainer review
remain authoritative. WASM budget caps are unchanged and owned by the lead.
