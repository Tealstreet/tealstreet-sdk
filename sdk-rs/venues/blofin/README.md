# BloFin socket liveness

Both feeds use the engine-managed lifecycle with independent epochs and retries.
Public keeps connect 20 s, idle 20 s, ping 5 s and backoff cap 16 s. Private keeps a 10 s
connection deadline, separate 10 s authentication deadline, ping 5 s and idle 20 s.
Pongs refresh liveness but never extend authentication. Only definitive code 0
login proof enables subscriptions and private financial payload publication.

The unchanged Primary SessionAuth provider signs a fresh login per current
Opened. The signed login is never an interest or replayed on reconnect. Public
interests preserve refcounts and books5 sharing; sequence gaps request managed
repair. Retirement invalidates books before replacement rows. See
`docs/plans/RUST_BLOFIN_MANAGED_SOCKETS.md` and production-binding controls in
`tests/managed_sockets.rs`. Replay tapes are scripted evidence, not live account
connectivity evidence.

Dispatched writes answered by HTTP 429, 408 or 5xx remain unknown through the
engine journal, regardless of a native refusal body. Native code `429` inside
HTTP 200 is also unknown, including a row with order ID `0` under an
`All operations failed` envelope: it cannot prove that nothing was placed or
authorize a protective put-back. Native rate failures direct the caller to
Check Open Orders and retain the venue code/message as diagnostic evidence.
Reads and documented definitive refusal codes on other statuses are unchanged.

Unknown native placement codes, including internal errors and duplicate client
IDs, also remain unknown under an `All operations failed` envelope with order
ID `0`. They cannot authorize a protective put-back. Uncertain placement rows
direct the caller to Check Open Orders; native diagnostics remain available.
Missing or contradictory success row codes also cannot authorize put-back.
Unknown cancellation rows and top-level placement/cancellation failures use the
same uncertainty guidance while retaining their native diagnostic evidence.
Only documented native refusal codes can authorize put-back; no synthetic text
marker is accepted from the wire. Blank, missing, and malformed write replies
remain unknown and direct the caller to Check Open Orders.

Leverage, margin-mode and position-mode writes use the same uncertainty guidance
when a native setting response proves neither success nor refusal, including a
blank code in HTTP 200. They retain native diagnostics, leave local settings
unchanged and never resend the POST. Documented setting refusals still name the
setting and its actual code/message.

Every failed write uses the dispatched attempt's classifier verdict before
choosing journal text or a fallback. Internal errors, blank codes and unlisted
codes retain Unknown guidance on HTTP 200 and HTTP 400, including native edits
that remove one combined TP/SL leg. An uncertain replacement or put-back never
authorizes another POST or claims the position is definitively unprotected.
This also applies to row-level errors under a success envelope: a blank row
code with unexpected failure text cannot acknowledge a cancel or native edit.

BloFin renders terminal write failures and warnings after the engine journal
chooses the final classification. Unknown writes always say Check Open Orders,
including transport loss and timeouts on settings, native stop edits, and combined
TP/SL leg removal. The original diagnostic remains in the message or native
diagnostic fields. Definite refusals keep their wording; an unknown response
never permits another POST. The trading table covers these failures for every
write path, including replacement and put-back dispatches.

Placement and cancellation batches classify envelope uncertainty before trusting
any row. Internal, blank, unlisted and rate-limit error envelopes make every row
Unknown, even a matching refusal, acceptance or already-gone row. They apply no
row acknowledgement to the Store and cannot authorize put-back or replacement.
Success code `0` and aggregate status codes `1`/`2` retain their row semantics.
