# KuCoin private-read diagnostics

SDK, NX and native callers receive the existing paced private-read notice with
the request path, `source=http|transport|signing`, `signError=true|false`, and
`HTTP=<status>|none`. Valid KuCoin envelopes retain their `code` and `msg` in
the message and the existing `venueCode` / `venueMessage` event fields.
`invalidSnapshot` distinguishes a rejected snapshot from a native refusal.

A local signing failure names the engine's closed signing error and
`signingStep=atomicGraph`. The provider's atomic result has no failing-operation
index; callers must not infer a request-MAC, passphrase-MAC or broker-MAC failure
from that label. Transport strings and malformed/raw response bodies are not
printed. Valid envelopes expose only KuCoin's error `msg`, never `data`, request
headers, credentials, signing inputs or returned MACs.

Retry scheduling and the existing IP-whitelist/auth classification remain in
place. Missing/empty passphrases refuse during construction; whitespace and
Unicode passphrases retain their UTF-8 bytes. Both old and new native paths use
key version 2 with a passphrase HMAC; this change adds no version-1 fallback.
