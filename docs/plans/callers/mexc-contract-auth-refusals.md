# MEXC contract refusal classification and read clock recovery

SDK, NX and hosted callers receive native refusal fields instead of a missing-list
error for success:false contract responses. Existing auth kinds and
auth.rejected drive the current key-refusal UI; native venueCode and
venueMessage remain available. No caller wire or deployment order changes.

602 remains a signature refusal and never grants a resend. The documented 513
clock-window refusal grants one unsigned server-time read and one exact signed GET
replay, retaining credential selection and original caller/background bounds.
Account polling and HTTP 429 pauses are preserved. No write or mutation lookup is
replayed. History retains its existing accepted-prefix/partial completion rules.

Deletion prerequisite: keep the typed envelope, original-caller settlement,
one-shot clock, expiry, invalid sample and no-write replay controls while these
read paths exist. Official sources and the request-time distinction are in the
venue README. No credentials, live probes or Sentry event payloads are included.
