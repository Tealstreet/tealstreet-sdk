# Bitunix secondary GET identity and egress

Reverified repository contract: Bitunix registry silently drops formed secondary
read credentials, its native signer always uses primary, and SDK transport lacks
the api-key discriminator. Fresh actual landing branch; keep public startup and
socket lifecycle changes separate until their actual merges require integration.

Add validated read_credentials to Bitunix Options with secret erasure; reject
partial/empty identities before startup I/O even for public-only engines. Registry
passes the validated blob. Select secondary only for signed Method::Get, keeping
mutations/posted reads/private WS login primary and public discovery unsigned.
Retain original query/body/entropy/timestamp/signature behavior. Temporary signer
copies must erase credentials on drop. Tag actual account/settings requests with
the selected credential slot and existing scope; history/public/trades are not
account-auth evidence. Native auth-result classification remains its own phase.

Add bitunix api-key to the SDK GET identity discriminator so identity and secondary
proxy/HOW egress agree. Use existing native serialized routing policy, with no
shared host runtime changes. Regress exact double-SHA signatures from actual
nonce/timestamp for GET+mutations, primary fallback and WS login, public unsigned,
malformed option early rejection, actual registry construction/first GET, wrapper
forwarding, SDK CORS/HOW/primary exceptions, and native plan equivalence.

Prove original failures; run Bitunix/Binance native, focused registry construction,
strict touched Clippy/fmt/bans, focused TS transport/parity tests, package typecheck,
lint/test gates as appropriate. Independent plan/implementation review, full CI,
real latest landing integration for shared registry changes and separate exact
guard before merge. Source merge hold for1637 remains active until release.

Plan and implementation reviews CLEAR, including final typed login-test decoder.
Original SDK transport cases fail without api-key support; actual registry GET
regression fails with forwarding disabled, and two signature tests fail with the
secondary signer disabled. Restored production passes 372 combined native tests,
actual registry construction, 33 native routing tests, 328 focused SDK tests plus
the matrix case, package typecheck/lint and strict touched Clippy/fmt/bans. Four
Bitunix routing cases added; all 70 existing fixture cases unchanged.

CI caught the native unit consumer's exact fixture count still pinned to 70.
Updated only that test assertion to 74; all 70 prior cases remain unchanged. The
actual native unit matrix consumer now passes, along with native-host strict
Clippy/fmt/bans; independent one-line test-delta review CLEAR.
