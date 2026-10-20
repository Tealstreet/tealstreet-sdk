# AsterDEX Rust port

Branch: `feat/rust-asterdex`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md`.
Full automatic execution; stable commits, no stash, PR targets the rewrite branch.

## Phases

1. Public data: crate, typed streamed market/ticker bootstrap, open-interest
   polling, aggregate ticker/mark streams, validated partial-book snapshots,
   candle reads/subscriptions, environment URLs and socket watchdog. Share
   identical Binance wire records, not Binance depth sequencing or policy.
2. Private data: auth, clock, listen-key lifecycle, account/position/mode/orders,
   per-market settings, split coalesced repair and stale-snapshot guards.
3. Trading: sequential regular/split/bracket orders, conditional close-position
   semantics, cancel/replace, idempotent unknown cancel, settings, builder fees
   and V3 STP. Reject caller client IDs and unsupported trailing inputs before I/O.
4. Queries: fills with the existing realized-PnL delta contract, order history,
   income and public trades in engine query shapes; typed response decoding.
5. Harness and gates: WASM/adapter/recorder registrations, independent auth
   oracle and documented private tapes (V1 and V3), recorded public fixture,
   strict correctness/performance and Binance regression, CI roster.

Each phase: audit plan, implement, verify, independent sub-agent code-correctness
review, fix, commit progress. Final: merge latest landing structure, run all
gates, open PR, post adversarial code-correctness reviews, address/repeat, wait
for green CI, merge, and cut a fresh follow-up branch.

## Authentication design

- Resolve explicit `extra.asterdexApiVersion` first; otherwise select V3 only
  for a valid user address, signer address and 32-byte private key. Preserve
  auth-field precedence over flat applicationId/key/secret. Wallet-shaped
  credentials missing the main wallet fail with the existing actionable hint.
- V1: `X-MBX-APIKEY`, business params then recvWindow=20000 and corrected
  timestamp, HMAC-SHA256. All signed verbs use query parameters, no body.
- Standard V3: canonicalize business params plus recvWindow/timestamp into
  sorted string-valued JSON. ABI encode `(string,address,address,uint256)`,
  hash with Keccak, EIP-191 hash those 32 bytes, recoverable secp256k1 signature.
  Append checksummed user/signer and a monotonic microsecond nonce to the query.
- Aster Code V3 (`POST order`, `GET builder/agent`): exact insertion-ordered,
  unencoded query, business params then asterChain/user/signer/nonce. EIP-712
  `Message(string msg)` under AsterSignTransaction v1, zero verifying contract,
  chain1666 mainnet/714 testnet. Signed bytes equal sent bytes before signature.
  Keep the TS split explicit even where newer docs describe EIP-712 generally.
- Reuse `safe-cex-crypto` Ethereum hash/sign/address and EIP-712 primitives.
  Keep venue ABI/preimage construction in the venue; no JSON value trees.
- Nonces increase even on equal/backward clock samples and across retries.
  Clock sync prefers selected version, falls back to the other, retains good
  offsets on failure, refreshes every five minutes. Only proven signature or
  timestamp rejection can resend a write; ambiguous writes never retry.
- Secondary read-only signer shares the owner wallet and selects separate
  read credentials/host egress; writes retain the primary signer. Inspect and
  extend current host secondary-key rejection narrowly, preserving defaults.
- Builder attribution is V3 only. Current mainnet default is the concrete
  builder address in TS, maker/taker rate0.0001; testnet placeholder disables
  it. Validate exact decimal strings (max0.001), zero-rate omission, per-order
  maker/taker selection and propagation to every child. Owner-wallet builder
  approval/agent registration remain onboarding operations, never synthesized.
- Independent tests must verify signatures/preimages using official documented
  examples and ethers, including decimal strings, testnet domains, field order,
  nonce monotonicity, missing/invalid keys, clock/rejection recovery and read/write
  separation. No real credentials enter fixtures or commits.

## Progress

- Planning: read latest brief/spec, parent and Aster guidance, inspected TS and
  Binance engine. Two independent read-only audits cover auth and venue surfaces.
- Public phase pending implementation. No gates claimed yet.
