# Crypto.com socket recovery

Market and user sockets have independent engine `Supervisor` instances with
10-second connection deadlines and 90-second receive watchdogs. Reconnects use
capped exponential backoff with deterministic jitter; short open/close flaps do
not reset the failure count.

The existing one-second delay before subscriptions/authentication remains.
Private login must succeed within 10 seconds of opening. A rejected login closes
immediately and retries after backoff. Heartbeats are answered with their native
request ID but cannot extend the authentication deadline. Retired sockets cannot
supply authentication acknowledgements to replacements, and duplicate successful
acknowledgements do not duplicate subscriptions. No active client ping is added.

Authentication requires an explicit numeric `code: 0` in the native reply.
Missing or malformed auth codes leave the login pending until its existing
deadline; heartbeat traffic still receives replies but cannot authenticate the
session or extend that deadline. Public data-envelope defaults are unchanged.

Candlestick subscriptions discard their entire first data response, including
the newest row, because it is a history snapshot. Only subsequent pushes enter
live candle frames. Reconnects, repairs, and a new subscription after the final
unsubscribe each discard history again. Requested history is available through
`FetchCandles`. Both live and fetched candles convert Crypto.com's millisecond
start times to Unix seconds, preserving fractional seconds.

Released writes answered with HTTP 408, 429 or 5xx remain `Unknown`, even when
the body carries a native refusal or signature code. Such a response cannot
authorize another physical attempt of that write or release its uncertainty
fence. Documented signature refusals at non-ambiguous statuses keep their bounded
retry; account and prerequisite read retries are unchanged.
