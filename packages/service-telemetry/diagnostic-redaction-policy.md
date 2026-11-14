# Diagnostic role redaction

`redactDiagnosticData` is the single policy for Requests Log, Loki, diagnostic
console, Sentry events/breadcrumbs, worker diagnostics and exchange-server logs.
No sink-specific switches are allowed. `diagnosticText` and `diagnosticConsole`
only format or forward copies produced by that function.

Credential roles use case-insensitive name substrings (sign/signature, hmac,
proof, auth, JWT, token, key, passphrase, session, challenge, cookie, password, seed and mnemonic)
and authenticated header families. Independently of field names, remove long
hex (40+ characters, excluding constant low-entropy runs), signature-length 0x values, PEM blocks, JWTs, and base64 /
base64url runs of 32+ characters with at least 3 bits/character entropy. UUIDs,
short IDs, numbers, symbols and prices remain useful context. Base64 decoding
is bounded at four levels, 64 tokens and 4096 characters/token; mixed binary
content never skips credential inspection. Valid prefixed JSON is copied and
scrubbed; ambiguous credential text is discarded.

Absolute and scheme-relative URL userinfo and credential query values are
scrubbed. Safe query context survives outside Sentry breadcrumb/exception and
request/Axios-context roles, which retain master's query-free route projection.
Existing redaction markers inside queries never become trailing URL punctuation.
Serialized SyntaxError / Sentry exception
records retain fixed metadata, never parser input excerpts. Copies never mutate
live signing, order or transport inputs, and diagnostic binary bodies are omitted.

Roles also preserve validated numeric CPU report counters only inside
`contexts.client_cpu_pressure`, and diagnostic correlation IDs in the fixed Loki
stream label schema. Breadcrumb request timing is scrubbed, event timing kept.
The standalone breadcrumb adapter supplies its schema role even without category.
Inspection-limit or host-object failures retain a fixed omitted breadcrumb, never
throw while extracting the redacted schema result.
The five exact top-level Sentry `tags.passphrase_*` shape fields retain only
master's closed string formats (length, boolean, autofill and entry mode).
The exact `key_check_path` and `signing_mode` tags likewise retain only the
existing add/engine/hosted and local/hosted_cosign/hosted_delegated enums.
Nested copies, alternate spellings and credential-shaped values remain redacted.
Encoded aliases cannot overwrite an exact safe tag. Structured Sentry
`contexts.request.url` keeps master's query-free request-location projection.
These rules live here and apply identically whichever sink invokes the function.

The venue audit fixtures cover every supported beta venue and alias, plus Bitrue's
retained signing carriers after SDK #3317 removed it from beta admission. Keep the
coverage check and output value-shape scanner: field-only assertions cannot
prove that compact or newly named signature fields are safe.

Console arguments cross the shared function as one argument vector, so a short
credential retains the role of its preceding header/field label even after a
context prefix. Do not redact arguments individually before spreading them.

Dynamic property names cross the same text/shape policy as values. Entries whose
keys contain credential material are omitted, avoiding collisions between
multiple redacted keys; input maps and safe entries remain unchanged.

Parser context is collected before copying sibling fields, so a process handler's
`reason: error.message` and Sentry `extra.reason` cannot recover a SyntaxError
excerpt. Discovery includes non-enumerable Error causes and AggregateError members,
using the same cycle and serialized-work bounds as enumerable children.
Fixed parser metadata is stable across repeated passes and carries no
stack excerpt. Validated source coordinates inside a non-parser stack retain
frame structure, while their text still crosses value-shape and URL redaction.

Serialized parser context is pre-collected before sibling projections, independent
of property order. Prefix text cannot hide a later valid serialized record.
Discovery is bounded to four serialized levels, 64 parses and 65,536 characters
per candidate; exceeding a bound returns the existing omitted-diagnostic marker
rather than copying text whose parser provenance could not be established.
Bracketed or brace-bearing prefixes cannot hide a later serialized parser record:
each candidate JSON start consumes that same shared parse budget.
Discovery reads complete balanced JSON fragments, respecting quoted brackets and
escapes, so trailing log text or adjacent records cannot hide parser context.
Already-parsed fragments are skipped; nested records are inspected recursively.

The local exchange policy additionally treats case-insensitive `Authent*`, JWT,
and session field/header names as credentials (including GRVT's
`x-ts-grvt-session`). Auth/login operations discard positional `args`/`params`.
Serialized JSON crosses the same object policy, including nested JSON strings;
ambiguous prefixed or truncated credential text is discarded. This includes an
API-key/secret/passphrase assignment whose closing quote was clipped; only an
already-redacted value may remain after the shared string scrubber. Unlabelled
base64url JSON/JWT strings and clipped prefixes are discarded conservatively,
before downstream length limits can retain part of a token. Property names cross
the same text projection before object serialization can clip a credential. Keep the final
Requests Log and Loki sink regressions for the actual exchange auth shapes.

Text key boundaries must not consume the separator needed by a following key
(`error:apiKey=`). Ambiguous auth text uses the same operation keys (including
`type`) and auth/login value predicate as structured frames, before any caps.

This is for diagnostic copies and persisted snapshots, never signing inputs or
the live trading transport. Keep timestamps and ordinary IDs intact. Callers
that need a stronger schema should project only their known fields first.
