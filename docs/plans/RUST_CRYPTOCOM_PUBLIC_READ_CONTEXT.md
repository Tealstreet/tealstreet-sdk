# Crypto.com public candles and synchronous history Read custody

Owner codex-ih35js. Fresh branch `feat/rust-cryptocom-public-read-context` from
actual landing `6a6845c449104130043055f5d03273f910eab80e`. Plan first; no runtime
conversion, inventory prune or readiness is claimed by this checkpoint.

## Scope and exact map

The actual checker inventory contains27 fingerprints/30 occurrences. The next
workable group removes7 fingerprints/9 occurrences: five local history terminals
and four candle terminal occurrences sharing two lib.rs fingerprints. The shared
get sender remains because discovery uses it. Projected Crypto total20F/21O.
Fresh open-PR file lists show #1788 owns account.rs/trading.rs/tests/account.rs;
none of this slice's source/test files collide.

| Path | Fingerprint | Count | Disposition |
| --- | --- | ---: | --- |
| `venues/cryptocom/src/account.rs` | `83c6b712d19379913a85bf575a1a1834358d1b5f84d4c7a04a2f241487399a9c` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/account.rs` | `a77d14858a6b097716fd945cddafa2c827e671194bbe1122e95c60b698c56006` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/history.rs` | `073a9daacf5c0553bfb2650ddc8db7b942b5f6a0c8125261daff7479814953b7` | 1 | Held remote history: native retry/staging-reset composition; conservative witnesses remain with source |
| `venues/cryptocom/src/history.rs` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | Local zero limit/equal nanosecond bounds: Context done |
| `venues/cryptocom/src/history.rs` | `4d39b6c1f48b4fe3cd73a278a44738b56c2d22a36fa0a56df1692009069d7a69` | 1 | Local unrepresentable nanoseconds: Context fail |
| `venues/cryptocom/src/history.rs` | `5f1c224f8aeec44e64bda630f879b41c6706957631dee106263ed3ec48c1beb5` | 1 | Local unsupported client lookup: Context fail |
| `venues/cryptocom/src/history.rs` | `7698d5bd23a40574165507b4b54b5d093fb00701512d29250192f1345e4da553` | 1 | Held remote history: native retry/staging-reset composition; conservative witnesses remain with source |
| `venues/cryptocom/src/history.rs` | `868b4a9af72a4afc961dcd2ad5bbd3601cd82824b125da57ed4b80ec47fce000` | 1 | Local missing credentials: Context fail |
| `venues/cryptocom/src/history.rs` | `ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4` | 1 | Local invalid range: Context fail |
| `venues/cryptocom/src/history.rs` | `d702253b3d082dc8c7278bfe17bdb876eeda775fedd004ebaaeea6cdd16f24ad` | 2 | Held remote history: native retry/staging-reset composition; conservative witnesses remain with source |
| `venues/cryptocom/src/history.rs` | `dfb17bdf89f16befd8bb7566ac05d0bddf2980c6627b642f20e1f1b5a4859508` | 1 | Held remote history: native retry/staging-reset composition; conservative witnesses remain with source |
| `venues/cryptocom/src/identity.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Held private setup/async callback custody, coupled to #1788 and opaque identity conversion |
| `venues/cryptocom/src/identity.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held private setup/async callback custody, coupled to #1788 and opaque identity conversion |
| `venues/cryptocom/src/lib.rs` | `121dc8add2c9a7d1d8fd691023a64c9ebb736360f0c97363cfc66956b0ec1405` | 1 | Held discovery sender: account fixture collision in #1788; retained unchanged |
| `venues/cryptocom/src/lib.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Candle response success: exact Context done |
| `venues/cryptocom/src/lib.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 3 | Candle unsupported interval, response error and transport failure: exact Context fail |
| `venues/cryptocom/src/trading.rs` | `2b3480974c11d493d8859f3ed1ecc2ed3370363182f22e5394ca7b6ccfedb502` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `5bfc783b8505466a5fad6b439e8145ed6557a4b2bd22a1b4d7f66e531c80206b` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `6c857f43df6f903845b498ba957ba70aedefef5068f0c26206cfd87745bd4c2a` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `7707e9a5c95ee3fc74e58a5fb683b90895a4a030f729a56f55ae1752cd3a1a5f` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `8c1d78bb768fb392694ad91c49f963ab64ba87a71d151646bff0298b85a6b734` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `acacfde7f264666789ccf383262670eebe6988b188d3747f0388d825e3072f6d` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `f32eb32f491028d71654d8ec05bbdf592328cd7f7156d848a17d430553a77638` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |
| `venues/cryptocom/src/trading.rs` | `f577ab41ba475c27e97cb5fb09a7c4216a6b20a88defe331a0dc268f15dfc531` | 1 | Held source/test collision #1788; signed writes/account and conservative witnesses retain native ownership |

## Shared authority and native behavior

At each local history refusal/empty branch call genuine admit_http_command
directly with Read/None/20000ms/Never, then done_context or fail_context. At most
one pure policy declaration is shared; no venue issuer/finalizer wrapper, owner
map, numeric fallback or root timer. Retain unsupported lookup before credential
validation, credential validation before time validation, exact nanosecond bounds
and zero-limit/equal-bound [] after those checks. PublicTrades needs no credential
and retains its native remote behavior when it is not a local empty result.

Candles admit one command-owned Read Context, store it in Pending::Candles and
submit unsigned request_managed with the same GET URL, escaped instrument, native
interval mapping, optional exact start_ts/end_ts, empty headers and no body. The
native limit remains ignored. Stream ArrayAt::Path(result,data) on the managed
request. Its original20s bound begins before preparation; Never adds no retry.
Validate exact delivery before staging streamed rows and before response parsing;
finish only through the original Context. Typed preparation and released HTTP
failures preserve their cause and bounded native metadata through the existing
engine errors. The established public-read NotSent outcome follows the standalone
book consumer; initial admission refusal keeps outcome None. Native candle codec
and response error messages, invalid-row handling, timestamp normalization and
ordering remain unchanged. Preserve typed terminal metadata, with no auth-health
evidence because no credential was carried; public-auth-health #1923 is actual.
No per-venue code/status ladder or text retry rule is added.

Discovery, account, private identity/Options, signing, trading and retrying history
remain unchanged. A complete discovery conversion needs the held account fixture
partial_catalogue_commits_then_rereads_without_a_second_private_start; filtering
managed SetTimer effects to bypass that assertion is forbidden. No signer recipe
or new shared API is needed for this unsigned/local slice.

## Original controls and reuse

Preserve prior work by plain-merging the existing test-first branch history,
never cherry-picking or copying a runtime diff. First integrate test-only
checkpoint b89ca9aaa52 (after cc2916a8037) onto this committed plan; it differs
from the actual Crypto source only by its two new test targets. Run those tests
on byte-exact actual production, record genuine assertion REDs and native working
controls, fix fixture/setup errors before counting REDs, and commit the receipt.
The old source checkpoint7d2a5b2eb60 and public-auth regression27b051a8907 may be
plain-merged only after independent plan review and the original RED checkpoint.
Replace its old finish_local_history issuer wrapper with direct shared entry in
each selected branch; that draft has never been published or claimed ready.

Required controls: invalid-clock and full genuine-owner capacity for local results
and candles; retained peer preparations and slot reuse; no I/O/timer/auth/Store
change on local refusals; exact native precedence and interval error; unsigned
Public routing/wire equivalence; parked route then equality/late timeout; stale
response, item, failure and bind after same-ID reuse; real released timeout versus
transport failure; cancellation cleanup; malformed streamed row/envelope; typed
route AuthInvalid/Expired/Permission with native metadata but zero auth-health
evidence; no automatic retries. Existing public candle/book/account financial
assertions remain, with only owned tests/public.rs preparation glue if needed.

## Gates and callers

Run Crypto.com plus Binance native suites, scoped all-target/all-feature strict
Clippy/fmt, both request-context checks and full bans with jobs2. Prune exactly
the seven keys/nine occurrences above, preserving every unowned map and shared
discovery sender. Add CALLERS/AUDITS/QUALITY implementation appendices and three
independent source/adversarial, caller and full-map reviews. CI owns complete
WASM/replay/CPU/size/memory certification. Draft PR into feat/rust-rewrite, no
merge without an explicit window.

Callers keep identical candle data, interval mapping and local history results
when admitted. Initial invalid clock/capacity instead returns the typed shared
Read refusal with outcome None. Async public reads retain typed transport/timeout
metadata and their original20s bound through route preparation, exactly once
across late callbacks and command-ID reuse. This changes no private auth evidence,
SDK capability dispatch, discovery cadence or history retry policy.

Architecture gate: No. The adapter consumes existing shared entry, immutable
Context, managed delivery validation and terminal authority. It adds no venue
issuer/finalizer, retry scheduler, classifier or auth-health workaround.
