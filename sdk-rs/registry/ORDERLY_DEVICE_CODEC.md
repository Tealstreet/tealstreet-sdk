# Orderly device codec (admission closed)

The `orderly-ed25519` wire format prepares one atomic Present(primary
credential) + Sign(Ed25519, primary signing ref) graph. It is a device codec,
not a venue admission declaration: `hosted_signing::admitted` and the
construction remote-signing policies remain false for Orderly and all four
aliases. The ordinary Orderly feature does not enable `hosted-signing`.
Native remote-provider selection and injected registry construction are later
slices. Do not enable hosted accounts based only on the format's existence.

A descriptor is a closed JSON object:

```json
{
  "version": "orderly/1",
  "venue": "mode",
  "slot": "primary",
  "kind": "http",
  "stampMs": 1700000000123,
  "payload": "1700000000123GET/v1/positions"
}
```

All fields are required. `venue` must match the device engine's bound Orderly
name (`kodiakfi`, `mode`, `what`, `woofi`, or internal `orderly`). The signing
refs come exclusively from that engine's primary identity; descriptors cannot
supply refs, keys, an account override, secondary slots or an algorithm.
The enclosing account router must select the saved device engine; this format
does not independently prove a server's accountName or account ID.

`kind: http` signs exact UTF-8 decimal timestamp + uppercase
GET/POST/PUT/DELETE + path/query + existing body bytes. `kind: wsAuth` signs
exactly the decimal timestamp. Timestamps must be nonnegative safe whole
milliseconds, must match the payload and, for a hosted HTTP capture, must
match its receipt. No nonce, passphrase or recv_window enters this graph.

Hosted HTTP inputs carry the descriptor as `payload`, plus the existing clock
stamp/revision. A `wsAuth` input with `outputFormat: orderly-ed25519` carries
the same descriptor as `toSign`, for either HTTP or WS signing work. This is
the device transport envelope, not a venue WebSocket request. The formatter
returns credential/signature directly for HTTP captures; the wsAuth wire
returns a JSON string containing those two fields. The credential is an
`ed25519:` public key decoding to 32 bytes; the raw signature must be 64 bytes.
Signatures use padded URL-safe base64, preserving the existing TS/native bytes.
No result headers are accepted.

Preparation and formatting perform no transport. Shared Capture retirement
fences late results, changed clock revisions, cancellation, timeout and
second completion. A rejected signing attempt remains NotSent; it never
replays a dispatched write. Final host admission must additionally prove the
clock-age checks and account routing in a real server/device round trip.

Frozen browser-oracle goldens live in
`packages/safe-cex/src/__tests__/fixtures/orderly-device-signing.json`.
The tests use synthetic provider material and cover all aliases, REST verbs,
WS auth, corrected clocks, malformed descriptors/results and closed admission.
