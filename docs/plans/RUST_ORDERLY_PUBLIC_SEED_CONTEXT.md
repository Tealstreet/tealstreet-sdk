# Orderly public seed Background Read custody

## Plan-only checkpoint and exact scope

Base actual9a757d406b8340b23dac71e20a35ac151de3956a. Branch
feat/rust-orderly-public-seed-context, owner codex-5c57jl. This checkpoint changes
only plan/index/new dedicated tests. Orderly lib.rs/sign.rs/Cargo.toml and its
5b recipe target remain byte-exact. Production explicitly waits actual combo-5b
and fresh owner composition review; no key-selection/nonce change is proposed.
This is planning work while #2051/#2056/#1930/#2031 wait on CI/review.

Migrate ONE shared unsigned sender, lib.rs::seed cx.request_streamed, fingerprint
dee025771565e025cbca449b90666e5c20dc3c0af9834094852eae869ee83f0a,
count1. It emits GET /v1/public/info (Markets) or /v1/public/futures (Tickers)
under the selected mainnet/testnet REST base, empty headers/no body, streaming
at data.rows. No credentials or signature apply to either kind.
Base request inventory Orderly28F/29O ->27F/28O, lib7F/7O ->6F/6O. After both
separate actual #2051 and #2056 landings, the composed count is24F/25O ->23F/24O.
Neither earlier PR is claimed landed or included on this base. No lexical
conservative format references are disguised as physical request adoption.

## Native coupling to preserve

Markets validates/stages rows, reconstructs exact native symbol map, replaces
Store markets and loaded flag, succeeds its Backoff, invokes account_start and
then issues Tickers. Tickers stages native quotes, replaces Store tickers,
succeeds its own Backoff, connects public socket and starts native funding.
Keep this order and native row filtering, market identity/duplicate checks,
empty/unusable response handling, warnings and every account/socket/funding
hook exactly. Private startup, signed account/history/trading, candles and book
snapshot wire/reducers remain outside the slice. Broker choice and both REST
hosts retain native construction, with no selection or key setup changes.

Both failures continue through existing seed_backoff per Seed, retaining native
jitter/retry progression. Each actual retry receives a fresh single physical
Background Read; there is no command receipt/terminal, mutation fact, Plan,
coalescing policy, batch-aggregate owner or shared new API. Preserve native
independent outstanding reads; do not silently impose latest-only generation
or coalescing semantics on sibling reads.

## Later implementation contract

Existing http_context Scope::Background, keyless Read policy, Never retry,
no pacing; use existing managed streamed GET transport and data.rows decoder.
Pending Markets/Tickers carry their exact Context. All success/body/item
staging/publication and global response classification must require genuine
validate_background_read_http_delivery before native decoding or effects.
Retire the exact Context before starting the next native seed/hook. Other
Pending families and their existing typed/read guards stay exact, including
5b signer/provider receipt handling once actually landed.

Declare one explicit NEW20-second admission-to-expiry bound for each seed leg,
including time waiting for Prepare; no renewal at Prepare/Bind/Claim or item
arrival. This changes previously unbounded native physical lifetime and must
be documented in CALLERS, not described as native timing parity. The markets
and tickers stages have independent admission origins, never one20s chain cap.

Typed Timeout/preparation/transport/queue refusal cleanup is failure-only:
consume/retire the exact returned Context and run existing native per-Seed
backoff once. Do not demand live DATA authority to perform expired-current
cleanup, publish on an expired response, leak a Context or strand startup.
Stale/duplicate physical ingress is inert; failed former legs must not lend
publication or command authority. Cleanup Cancel is allowed and is not IO.
Actual queue refusal returns its Pending for matching cleanup; an admission
refusal before Pending exists backs off that native Seed without a fabricated
Context or recursive retry. No local copied Context/issuer/attempt validator,
no extra venue timer, signing, key fallback or read-selection rule.

## Original REDs and native controls

New tests/public_seed_context.rs on unchanged source. Separate markets/tickers
Prepare-before-raw-release and exact final-expiry publication controls, plus
an unprepared market admission-timer/late-Prepare control. Deeper suffixes
following the first failing assertion remain mandatory source-stage gates,
not independently executed original evidence. Full action batches distinguish
cleanup Cancel from new physical IO and do not lend command/signing rights.

Working controls pin exact wire and market/quote results on both deployments,
Markets->Tickers order and loaded flags, no signing/command terminals, actual
network failure/backoff/recovery for each Seed, and native empty/unusable rows.
Tests use existing native catalogue/ticker payloads and real Engine startup.
No old test assertions are changed. After actual5b/source adoption, add genuine
Context/queue pressure, preparation/typed Timeout cleanup, streamed item/final
expiry, duplicate/retired callbacks and credit recovery controls before freeze.

## Workflow gates

Plan commit precedes original test commit. Only scoped Cargo with
CARGO_BUILD_JOBS=4, one own build at a time; logs outside Git. Record executed
assertion REDs apart from fixture/setup errors and native GREEN controls.
Obtain bounded core/native-caller review on immutable pins. Do not touch any5b
source or publish production from this checkpoint. Future source/actual5b
composition, exact inventory pruning/foreign preservation, scoped native and
strict checks, fresh CI/size/performance and train GO remain separate gates.
