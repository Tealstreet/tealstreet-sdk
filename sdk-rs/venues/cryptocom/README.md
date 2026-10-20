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
