# Hosted account identity

Private hosted accounts use the authenticated user, venue namespace and canonical
public credential to select an existing account actor. An account name is a
connection-local label. Renaming a label does not create another actor; another
credential or user selects another identity. Public-only accounts retain their
existing registry and wire behavior.

This extends the account registry, holders, settings actions, stop notices and
native TWAP owner binding introduced by #3427. It adds no session, engine,
transport, signer or order retry mechanism. It enables no DELEGATED venue.
Canonical identity is separate from possession proof. The shared admission
rollout (#3494) and its per-venue readiness rules remain deployment prerequisites;
this change does not declare another venue proof-ready.

## Public init fields

The existing `key`, venue and network fields remain. A private init must identify
the public credential used by its signer:

| Venue | Identity inputs |
| --- | --- |
| API-key venues, including Coinbase | `key`, preserving case and applying only the primary credential trimming declared by construction rules |
| Backpack | `key`, a valid base64 Ed25519 public key |
| Hyperliquid and Hyperliquid spot | `key` for the main wallet, new `signerAddress` for the agent, and optional `vault` |
| GRVT | existing `grvtTradingAccountId` and `grvtSignerAddress` |
| Paradex | existing `paradexAddress` and new `paradexPublicKey` |
| Lighter and Lighter Robinhood | account index, `apiKeyIndex`, and the current registered public key obtained by the server |

Ethereum addresses use their canonical lowercase 20-byte form. GRVT's trading
account is a decimal uint64. Paradex account addresses and Stark public keys use
canonical 32-byte hex; the public key must be a valid Stark curve public value.
These fields are public values; no additional secret enters a COSIGNED init.
The device obtains signer public values through its existing local provider.

Lighter's explicit account index is the existing `applicationId`; its API key
index is in 0..=254. Without an explicit account index, the server uses the public
L1 address selector and existing account lookup endpoints. An ambiguous account
list requires an explicit index. The API key index is never an account selector.
The server reads the registered public key anew for each init, including a
repeated label. Re-registering a slot with another public key selects a distinct
identity. Client JSON cannot supply the server's registered-key evidence.

Metadata work uses the existing routed native transport and bounded public-read
helper: 16 KiB maximum and the existing 20-second deadline per request. Address
selection tries at most two account lookup endpoints before the registered-key
read. Missing, ambiguous, malformed, failed or stale evidence refuses attachment.
The pending task retains its original user, socket and generation; its original
token deadline and lease generation also fence deferred actor publication.

## Namespaces and settings

Sibling products share the venue family namespace: Bybit, Binance, OKX, Bitget,
KuCoin, Hyperliquid, BingX and Coinbase variants each use their family. Mainnet,
testnet and demo namespaces follow the native venue routing. Bybit demo with
`testnet: true` uses ordinary Bybit testnet. OKX testnet and demo aliases share
the simulated-trading namespace. OKX EU, Bybit regional sites and the Lighter
Robinhood deployment remain separate namespaces.

The identity is an unsalted, domain-separated SHA-256 digest of length-delimited
user, scope and canonical public key ID. Existing Bybit identities keep their
published vector. Raw public credentials are not used as registry names or logged
by the identity resolver. Settings and mode checks remain separate: sharing a
public identity does not permit an implicit settings change or attach another
signing mode. Existing explicit settings actions retain their lifecycle.

## Signing selection hint

The server's existing `sign-req` adds the public `keyId` selection hint to other
hosted venues, as #3427 already does for Bybit. Its value is the canonical public
key ID: an API key string, canonical Backpack public key, or a JSON-encoded tuple
for wallet venues. This hint supplies neither authentication nor permission.
The existing exchange name, socket ownership and account registration still bind
which device may answer a request.

Wallet tuples are ordered as follows:

- Hyperliquid: main wallet, signer address, vault (empty for the main wallet).
- GRVT: decimal trading account, signer address.
- Paradex: account address, Stark public key.
- Lighter: decimal account index, decimal API key index, registered public key hex.

The additive native `remote_account_with_key_id` constructor accepts that hint.
The existing `remote_account` constructor retains its original behavior.

## NEEDS-NX

The hosted init builder must send `signerAddress` for Hyperliquid and
`paradexPublicKey` for Paradex, obtained from the account's existing trusted device
provider. GRVT must supply its existing public signer and trading-account fields.
Lighter may supply its known account index to avoid an ambiguous address lookup.
Host protocol typings need these public additions; no generated SDK exchange
option or TypeScript account type is changed by this server contract.

Hosts keep account names as labels and handle `verifying` while public identity
resolution is pending. A signing responder that uses `keyId` must compare the
canonical public credential within the current user and venue/network scope;
it must not select a secret across accounts merely because a label matches.
The same metadata requirement applies to an explicit settings request.

Native TWAP state must retain its identity `ownerId` when an account label changes.
A legacy record without that owner cannot be attributed from its label and is
refused by the existing owner fence; hosts must not automatically replay it.

Order submit, acknowledgement, outcome and receipt formats are unchanged.
An unknown write outcome is never re-sent by this work. Identity or metadata
refusal does not authorize replay of an earlier write.
