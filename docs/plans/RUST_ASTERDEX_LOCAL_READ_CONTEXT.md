# AsterDEX synchronous Read custody

Owner codex-ih35js; plan first on actual landing
`6c7f4fcf292d01b8c056127a070783e3fd72dcd2`, branch
`feat/rust-aster-local-read-context`. Phemex belongs to the #1974 fold.
Plan and original controls precede the runtime checkpoint. The plan branch now
plain-merges actual caller-root landing `6a6845c4491`; the local Read slice uses
existing entry authority independently of that API.

## Actual lane remainder

The actual checker manifest and scanner agree. Fingerprints are path-qualified;
occurrences include conservative lexical witnesses, not only HTTP sends.

| Venue | Fingerprints | Occurrences | Workable occurrences | Blocked occurrences |
| --- | ---: | ---: | ---: | ---: |
| AsterDEX | 43 | 67 | 13 | 54 |
| MEXC | 21 | 21 | 5 | 16 |
| WOO | 31 | 34 | 0 | 34 |
| Crypto.com | 27 | 30 | 9 | 21 |
| Total | 122 | 152 | 27 | 125 |

Aster has10 synchronous Read terminals selected below and3 separately workable
pre-I/O trading refusals. Those three also serve FetchStpMode Read commands:
they require explicit operation propagation, and placement-capacity refusal
follows acknowledged.retain today. They are not a Mutation-only helper task.
The remaining54 comprise28 runtime and26 conservative occurrences. Their
shared request funnel is held on the core-owned exact1000/2000/3000ms retry
wakeups/four attempts and clock-held custody; recipe/egress availability does
not prove that composition. Private signature and timestamp corrections can
each resend once, whereas the write journal grants one classified resend per
logical leg: its owner must settle that composition before write adoption.
The clock send is included in held pending exact issuer/deadline/release proof,
per codex-uufub4's fresh ruling. Paging alone does not need the closed #1928;
ReadCompletion1848 and caller-root1976 are the relevant bridge APIs. No obsolete
signer, scoped-egress or Plan-history blocker is claimed.

MEXC workable5 consists of unsigned background discovery/depth sends2 and a
separate local trading refusal slice:2 reachable Mutation branches plus1 adjacent
unreachable Err terminal. Its16 held sites comprise11 signed sends/async
terminals and5 conservative witnesses. #1962/#1968 are actual, but no reviewed
MEXC dispatch recipe or assigned signer owner exists; identity finish hashes are
shared with valid async callbacks and cannot disappear via local-only admission.

WOO has27 occurrences in files held by #1788(account/trading) and
#1930(lib/account). The other7(history2,identity2,secondary tests3) remain
coupled to the held callback/signer funnel. There is no WOO dispatch recipe;
claude-4yjlw0 queues it after BloFin. Unsigned Primary command entry also has no
reviewed admit_http_command_with_egress pin; scoped issuance is not a substitute.
Secondary test witnesses are not three physical HTTP requests.

Crypto.com workable9 consists of local history5 and complete unsigned candles4.
Its21 blocked occurrences comprise13 in #1788-held account/trading files,
discovery sender1 requiring its held account fixture,5 retry-bearing history/
conservative sites and2 identity
finish sites coupled to private setup/callback custody. Native history retries
wait1000ms and reset only the failed staging; no copied retry scheduler is
allowed. The Envelope recipe, shared selected-read helper and public-auth-health
fix are actual. Discovery has no missing engine API, but its account fixture
partial_catalogue_commits_then_rereads_without_a_second_private_start rejects
all SetTimer actions, including the managed deadline; its raw-drain Harness also
needs owned preparation. That test file is held by #1788, so this is a collision,
not an API blocker, and timer filtering must not hide it.
The old #1923 auth-health blocker is resolved, not a reason to
hold unsigned/local work. The existing local Crypto plan/source branch can be
plain-merged later as authorized; no cherry-pick or source-copy shortcut is used.

## Selected largest coherent local Read group

Ten occurrences in history.rs and lib.rs can consume existing genuine entry
and Context completion without signing, HTTP, timers, sessions or #1976.
codex-cxxyqr explicitly released the seven history terminals and has no active
or unpublished history.rs source/tests. Fresh open-PR file lists contain no
Aster source/test collision. Other Aster request/account/sign/trading code stays
outside this slice.

| Path/native terminal | Fingerprint | Removed occurrences |
| --- | --- | ---: |
| history.rs: keyless private, missing fills symbol, V1 unscoped orders ->[] | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 3 |
| history.rs: invalid range / unrepresentable inclusive end | `99dc418d7a4be445d25b3f090a9e64b85bffe6ec652ba8dd748b583186d68b22` | 2 |
| history.rs: order history wider than seven days | `ef2650178fae03a9600bb28a3783c7366361657cbff6bd24a137587b1d558c9b` | 1 |
| history.rs: unsupported client-ID lookup | `00e86f5d65c7a11874e2adf9c5e2ff0823ed9665f8956ff6e8c3fba14d3f6182` | 1 |
| lib.rs: unsupported candle interval | `0c84f26574bbfb5bb1c4dce587925cebfaad0e356b6245aebd577a633da18c30` | 1 |
| lib.rs: cached leverage success | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 |
| lib.rs: cached leverage failure | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 |

Seven path/fingerprint entries are touched; five vanish completely because the
last two lib fingerprints still serve remote callbacks. Exact projected shrink:
Aster43F/67O ->38F/57O; whole lane122F/152O ->117F/142O. No macro-only rename,
checker exception or remote-site retirement is claimed.

## Admission and native precedence

At each selected synchronous branch, invoke Cx::admit_http_command with Read,
credentialNone, timeout20000ms, pacingNone, RetryPolicy::Never, then finish only
through that Context. Rejected is already terminal; Revoked grants no fallback.
Use the engine directly, with at most one pure policy declaration, no venue
issuer/owner map, no numeric-id completion wrapper and no fresh Context inside
callbacks. Successful remote paths retain their existing exact expressions.

Preserve keyless private[] before every range/lookup check, with PublicTrades
exempt. Preserve invalid/negative/nonfinite/reversed bounds, equal income bounds,
seven-day order cap, unsupported lookup, missing fills symbol and the inclusive
end precision check in their current order. V1 missing order symbol remains[];
V3's remote unscoped request stays native. Cached leverage still requires finite
positive Store leverage and emits the exact native JSON/error. Unsupported candle
interval retains interval.unsupported("AsterDEX") before request construction.
Local output/error kinds/outcomes remain native when admitted; invalid clock or
capacity instead emits the sole canonical typed Read admission refusal with no
financial outcome, I/O, timer, diagnostic, Store mutation or peer completion.

## Original controls and verification

Commit original executable assertion REDs before source. Drive actual Engine
entry with trusted Aster fixture options, perps V1/V3, testnet and keyless states.
Pin all selected branches at invalid clocks and full capacity, every retained
peer Context still live, duplicate active ID isolation, finite ID reuse and more
than256 sequential synchronous retirements. Native working controls pin exact
empty/refusal ordering, cached leverage success/error JSON, PublicTrades keyless
remote exemption, and valid V1/V3 remote reads remaining on their original path.
A small test Venue wrapper may delegate actual Aster command callbacks while
minting retained genuine peer roots through independent real command entries;
it must not fabricate Contexts, numeric failure custody or private engine state.

After three independent plan reviews, implement direct shared admission and
Context terminals; prune only the ten actual occurrences. Preserve all existing
Aster financial/signature/history assertions. Run touched Aster + Binance native
suites with CARGO_BUILD_JOBS=2, strict scoped all-target/all-feature Clippy, fmt,
both request-context checks and bans. Add CALLERS/AUDITS/QUALITY in the same
implementation PR. Independent source/adversarial, caller and full-map reviews
must answer the architecture gate. CI owns family WASM/replay/CPU/size/memory.
Open a draft PR into feat/rust-rewrite; no landing merge without explicit GO.

Architecture gate: No per-venue replacement for a cross-cutting concern. This
slice only consumes existing shared entry/refusal and Context terminal authority.
Clock, delayed retry, read walks and write-journal composition remain with their
shared owners. No unresolved remote prerequisite is bypassed by this local work.

## Original assertion checkpoint

On unchanged actual `6a6845c4491` Aster production, the new actual-Engine target
`request_context_local_reads` compiles and executes26 controls:20 assertion REDs
(ten selected terminals times invalid clock/full capacity), six native working
positives, zero ignored. The positives pin V1/V3 local precedence, exact local
results on mainnet/testnet, cached nonpositive/nonfinite error, more than256
synchronous retirements with ID reuse, live duplicate-owner isolation and its
shared diagnostic, and keyless PublicTrades remaining remote. The full-capacity
fixture retains256 genuine Read roots, checks public root validation around the
callback, and frees one by its actual timer before retrying the selected command.
No source or inventory changes are part of this checkpoint.
