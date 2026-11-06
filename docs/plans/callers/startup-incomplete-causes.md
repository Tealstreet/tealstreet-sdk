# Startup incomplete cause attribution

`startup.incomplete` retains its existing message, severity, 60-second first
deadline and five-minute repeat cadence. The engine adds fields for each missing,
required section. No loaded or non-required section contributes fields.

For `X` in `Markets`, `Tickers`, `Balance`, `Orders`, `Positions`:

| Engine field | Contract |
| --- | --- |
| `startupXState` | `neverStarted`, `started`, `timedOut`, `refused` or `failed` |
| `startupXKind` | The existing closed `ErrorKind`, when known |
| `startupXVenueCode` | Parsed native code, at most 32 ASCII letters/digits or `_:-.` |
| `startupXHttpStatus` | This failed read's observed integer HTTP status, 100–599 |

`started` means that a declared read was issued, including managed preparation;
it does not claim physical network release. `neverStarted` means no declared
read issuance or explicit refusal was observed. A retry preserves the last
failure until another failure replaces it. Missing kind/code/status remain
absent: untyped native failures do not acquire an invented native classification,
and preparation failures do not borrow an unrelated response's HTTP status.

`Venue::startup_reads` labels native pending variants at issuance. Transparent
native wrappers delegate to their inner request. Catalogue/ticker joins and
account snapshots explicitly identify their own sections; commands, candle
reads and optional metadata refreshes return `Sections::NONE`. A new startup
request must declare membership here. Shared engine transport, timeout and
preparation failures retain their typed causes. Native read failure notices and
the shared public retry helper retain already-classified venue failures; Bybit's
text-only public callback records its existing typed read refusal before reducing
it to text, including native re-sign retries. Local refusals without a request id
use the same explicit membership.

The tracker has five slots and no retained request payload, URL, account, order
identity, raw error message or response body. Issuing follow-up work does not change the active response callback's section or
HTTP status. Its later asynchronous response has its own immutable membership;
caller-owned reads and lookups cannot overwrite startup causes. Account prerequisites
that gate the private snapshot set explicitly label the sections they block. There are no additional logs,
timers, retries or changes to request/terminal authority.

`BaseExchange.emitAndLogError` projects only the bounded event fields to
`startup_<section>_{state,kind,venue_code,http_status}` Sentry tags. All keys are
at most 32 characters. The hosted server's existing event forwarding carries
the fields unchanged; the browser needs no local startup-read state. The
existing worker sanitization and main-thread Sentry sink accept these tags;
no NX allowlist change is required.

Controls exercise missing/required sections, a real managed preparation refusal,
native HTTP rejection, transport failure, exact physical timeout, retry retention,
unrelated-read isolation, a real Bybit native rate-limit envelope, and a
hosted-shaped fields-only event through the real TS error-reporting boundary.

Closed startup states/kinds use borrowed static log values, preserving the existing
watchdog allocation pin. WASM and hosted serialization is byte-identical to owned
strings; text-value equality and the existing log-size limit apply to both.
