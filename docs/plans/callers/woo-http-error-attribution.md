# WOO account-read failure attribution

Account-read callers should use the typed `kind` and `venueCode` fields. The
message includes HTTP status and an integer WOO code when present. Transport
failures have no HTTP status or venue code. Response bodies, venue messages and
raw transport text are excluded from these account diagnostics.

Only auth statuses or WOO auth codes identify an auth refusal. Endpoint absence,
rate limits, server failures, malformed responses and transport failures retain
their own kinds; they do not deactivate the account or end startup as a bad key.
The existing retry cadence, account cleanup and all write classification remain.

## Separate V3 migration follow-up

Unauthenticated GET probes on 2026-11-04 at 01:19:53 UTC returned HTTP 401 and
WOO code -1002 for both `/v3/accountinfo` and `/v3/positions`, and for their new
counterparts `/v3/account/info` and `/v3/futures/positions`. No retirement or 404
response was observed. This is not proof that authenticated legacy calls succeed.

Use WOO's [migration guide](https://developer.woox.io/api-reference/migration_guide)
for a separate migration of REST paths, parameters and native response models.
Its [WebSocket guide](https://developer.woox.io/api-reference/websocket_introduction)
requires an authenticated listen-key request and `/v3/private?key=...`, replacing
the legacy application-ID socket and auth frame. Migrate the private event codecs
and public subscriptions together with their protocol tests. Existing synthetic
legacy tapes prove signing parity, not support for the new endpoints or live keys.

This change does not migrate endpoints or certify the reported user's key.
