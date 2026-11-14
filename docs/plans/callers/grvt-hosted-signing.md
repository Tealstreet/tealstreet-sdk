# GRVT hosted signing — HOLD for Sam

Keep this PR unmerged until Sam explicitly approves activation. This change
admits RemoteSigner only. Private PayloadSigner remains unsupported; the
user-facing admission messages in `betaVenues.ts` are unchanged.

## Findings and RED evidence

GRVT was already in the SDK roster, native registry, enabled exchange metadata
and WASM family tables. The failing message was the delegated-account refusal:
`assertBetaAccount` could not find an enabled `remoteSigning` declaration.
`rust.grvt-hosted-signing.test.ts` failed unmocked on that missing declaration
and on GRVT advertising HMAC rather than secp256k1 capabilities.

Admission alone was insufficient. GRVT's provider recipes existed but were
not installed by production construction. `registry/tests/grvt_hosted_signing.rs`
failed with `Unsupported` before the device codec was connected. Construction
now consumes raw credentials into the owned provider and installs opaque refs.
The standalone `Grvt::new` compatibility API remains for native callers and
local controls; registry and SDK construction use installed custody.

Five of the 61 installed-custody trading fixtures failed on protective
restore preflight. These RED cases proved that restore preflight requires a
wire shape before signing. An unsigned preview now supplies that comparison; it never
reaches transport. Restored client metadata is retained in the frozen order,
and the dispatch provider supplies and verifies the final signature.

## Protocol and endpoint inventory

| Native operation | Authentication and parity boundary |
| --- | --- |
| `POST /auth/api_key/login` | One Present of the installed primary API credential; identical JSON/login headers. Reauthentication repeats this operation. |
| `POST /lite/v1/create_order` | Atomic statement commitment, public-address proof and secp256k1 signature of the venue's EIP-712 digest. Used for ordinary orders, SL/TP, replacement and protective put-back. |
| Balance, positions, open orders, account identity, leverage/builder reads, history | Session cookie and account headers from login; no independent HMAC or signature. |
| `cancel_order`, `cancel_all_orders`, `order` terminal proof, `set_initial_leverage` | Same session headers. Amend is inspect/cancel/terminal-proof/replacement, not a separate signed endpoint. |
| Private WebSocket | Session authentication/upgrade and auth frame; no separate wallet or MAC signature. |
| Public market/ticker/book/candle/trade reads and streams | Unsigned. |

The remote provider accepts only the native GRVT graph. The device wire is one
`ws-auth` request with `outputFormat: grvt-order` and a closed JSON descriptor.
It contains canonical integer strings, asset ID, side, order flags, network,
subaccount, nonce and expiration. The device checks its configured subaccount,
network and clock, derives the builder/domain and digest itself, and signs with
its owned scalar. Bare digests and foreign account/network descriptors refuse.
The result is `0x` plus the 20-byte wallet address and 65-byte signature. Native
apply verifies the statement commitment, recovered address, digest and receipt
before rendering the request. Provider recovery IDs are 0/1; the existing wire
renderer produces GRVT's 27/28 bytes identically to local signing.

Hosted origins must configure `grvtSignerAddress`; devices must configure the
matching `grvtTradingAccountId` to sign orders. The configured address is checked
against every returned signature, never silently replaced. API credentials are
used only for login. Private scalar material and opaque key refs never cross
the delegated wire. Secondary signing is unsupported. Local construction still
derives its signer address from custody and may discover its trading subaccount.

This GRVT lane has no signed receive-window field or REST clock-sync operation.
Nonce and one-hour expiration are frozen once with each order. Route re-preparation signs
the same digest and never renews that nonce or lifetime. Device clock checks
allow at most 30 seconds of skew; expiration still refuses. Existing provider
recipe tests exercise stale route re-preparation, asynchronous re-signing,
expiry and wrong receipt/digest/address/provider-header failures. The GRVT
  capture regression also retires an old clock revision and proves re-signing
  at the corrected clock preserves the frozen nonce, expiry and signature. No fabricated
clock-sync HTTP endpoint or receive-window signature is claimed here.

## Deterministic evidence

- All existing GRVT native trading fixtures run through a parity provider that
  reconstructs the device graph and compares its address/signature bytes with
  separately provisioned local custody. This covers ordinary and protective
  writes, cancel/amend proofs, restores, reauthentication, errors and Unknown.
  Run the same trading fixtures with `GRVT_LEGACY_CONTROL=1` for the raw local control.
- Existing recipe tests independently compare final order bodies with the
  local-key signer on both networks and prove stale re-preparation preserves
  all frozen fields. The remaining native suites retain read, session, WS,
  history and failure-path controls.
- Actual WASM device fixtures compare twelve order vectors (two networks,
  boundary nonces, ordinary/reducing flags) with independent ethers EIP-712
  hashes/signatures. Cross-account/network, unknown-field and expired work
  refuse without signatures or material-bearing errors.
- An actual WASM local origin and a keyless native origin run the same canned
  startup and placement. Their complete HTTP packets are byte-identical,
  including login/session headers, body and signature. Transport loss remains
  Unknown with one physical create; a throwing delegate produces NotSent and
  no create. Provider graph/negative-admission/cancellation tests prevent a
  callback fallback and check error/wire redaction. No live calls are made.

The signing inventory changes only GRVT's existing compatibility fingerprints
and the new credential-erasure line; it adds no raw credential holder. Ordinary
and protective restore fixes share the provider path. Full checks and measured
WASM budget evidence belong in the PR description; CI and maintainer review
remain authoritative.

## Native remote-provider slice

The native `hosts/remote-signer` GRVT family now accepts only the owned
credential Present for login or the three-operation order graph described
above. Its capability probe reports secp256k1 only for its owned signing ref;
the operational interpreter ships with that declaration. Public-address setup
remains unsupported: GRVT uses its configured expected address and verifies
the address and recovered signature on every order.

Fixed public ethers/browser goldens cover both networks. Native link tests
cover malformed graphs/replies, cancellation, timeout, disconnect and late
replies without a second request. No local scalar or new crypto dependency is
installed on the server. The shared admission policy and WASM caps are unchanged.

NEEDS-NX: native GRVT provider capability is no longer the generic HMAC fallback;
update native hosted-refusal examples that assumed it could not sign. This
slice alone does not complete native hosted GRVT: server InitPayload still
needs signer-address forwarding, caller compatibility and a real server/device
round-trip proof in the subsequent slice. Keep activation frozen pending
non-author review and the existing caller coordination.

## Native server signer-address contract

Private GRVT `/account` init retains optional `grvtSignerAddress` alongside
`grvtTradingAccountId`. Both enter engine options and the account fingerprint;
changing either requires a rebuilt account. Only GRVT consumes the signer
address. Authenticated explicit-public accounts discard both fields before
construction, while unauthenticated public Bybit init rejects signer metadata.
The established text decoder still rejects object, array and boolean fields.

Hosted private GRVT requires a valid configured address. Missing metadata from
an old sender fails construction before transport; there is no inferred wallet
or scalar fallback. Public/default and ordinary API-key-only local session
construction remain available without wallet metadata. Native routing now
recognizes the venue's mainnet/testnet REST origins; native cookie and account
headers remain the session authority.

Canned native-host tests exercise server preparation, remote-account erasure,
registry construction and the real device wallet codec through `SignerLink`.
They cover read readiness, private subscriptions, one verified order, corrupt
proof refusal and an ambiguous write with no resend. The codec is checked
against slice 1's independent public browser goldens on both networks.

NEEDS-NX: the caller owner must prove serialization of `grvtSignerAddress` and
routing to the matching device account/subaccount. The sender is outside this
mirror and has not been verified here. Land the accepting server schema before
requiring it from callers; older servers discard this field and refuse private
GRVT construction. This deterministic host proof is not venue acceptance or a
live-key smoke. Keep the matrix partial and activation frozen until caller
compatibility, slice 3 qualification and non-author review are complete.
