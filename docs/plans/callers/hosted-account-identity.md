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
| Hyperliquid and Hyperliquid spot | `key` for the main wallet, the existing `hyperliquidSignerAddress` for the agent (see `public-key-admission.md`), and optional `vault` |
| GRVT | existing `grvtTradingAccountId` and `grvtSignerAddress` |
| Paradex | existing `paradexAddress` and `subkeyAddress` (the subkey's Stark public key, as in `/auth/{subkey}`) |

Ethereum addresses use their canonical lowercase 20-byte form. GRVT's trading
account is a decimal uint64. Paradex account addresses and Stark public keys use
canonical 32-byte hex; the public key must be a valid Stark curve public value.
These fields are public values; no additional secret enters a COSIGNED init.
The device obtains signer public values through its existing local provider.

A DELEGATED init seals the signing secret, so trusted construction derives the
signer (Hyperliquid, GRVT) or Stark public key (Paradex) from it and refuses an
init whose claimed field differs. A claim therefore cannot select another key's
actor. COSIGNED binds the same fields through its possession proof.

Lighter and Lighter Robinhood keep their existing `(userId, accountName)` label
identity. Their registered-key admission (`public-key-admission.md`) already
reads and pins the slot's registered Schnorr key during each join and cold build;
keying the actor by that key needs a single shared read and is not part of this
change.

## Namespaces and settings

Sibling products share the venue family namespace: Bybit, Binance, OKX, Bitget,
KuCoin, Hyperliquid, BingX and Coinbase variants each use their family. Mainnet,
testnet and demo namespaces follow the native venue routing. Bybit demo with
`testnet: true` uses ordinary Bybit testnet. OKX testnet and demo aliases share
the simulated-trading namespace. OKX EU and Bybit regional sites remain separate
namespaces. Binance Spot Test Network has
its [own API-key registration](https://testnet.binance.vision/) and is separate
from the derivatives demo namespace; their mainnet products retain their shared
family namespace. Registry actors and native TWAP owners additionally distinguish
the native product, so one credential can hold separate spot and derivatives
actors at the same time. Renamed labels for the same product still share an actor.

The identity is an unsalted, domain-separated SHA-256 digest of length-delimited
user, scope and canonical public key ID. Existing Bybit identities keep their
published vector. Sibling-product actor IDs qualify that credential digest with
the native product name in a separate `tealstreet/actor/v1` domain. OKX demo
aliases use the corresponding non-demo spelling (`okxdemo` to `okx`,
`okxeudemo` to `okxeu`); their effective network and site remain in credential
scope. Alias routing/settings differences still require the existing explicit
settings action rather than creating another actor. Each family's base product
retains the credential digest as its actor ID. Raw public credentials are not
used as registry names or logged. Settings and mode
checks remain separate: sharing a public identity does not permit an implicit
settings change or attach another signing mode. Existing explicit settings
actions retain their lifecycle.

GRVT's login API key is a settings input separate from its trading-account/signer
identity. Changing it requires the existing explicit settings action, which
rebuilds the login provider; ordinary re-init refuses the change. Its fingerprint
uses the existing process-salted digest rather than retaining the login key.
The same rule applies to an API passphrase captured by the remote provider:
correcting it requires explicit settings replacement, without changing public
identity. Only its process-salted digest enters settings comparison. KuCoin's
device-derived passphrase path remains unchanged.

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
- Paradex: account address, subkey Stark public key.

The additive native `remote_account_with_key_id` constructor accepts that hint.
The existing `remote_account` constructor retains its original behavior.

## NEEDS-NX

The hosted init builder must send `hyperliquidSignerAddress` for Hyperliquid,
derived from the agent secret (NX already does). Paradex uses the existing
`subkeyAddress` and `paradexAddress`, and GRVT its existing public signer and
trading-account fields, so no new init field is needed. Paradex stays LOCAL in
NX's `AWAITING_LIVE_TEST` until a live hosted test passes. No generated SDK
exchange option or TypeScript account type is changed by this server contract.

Hosts keep account names as labels. A signing responder that uses `keyId` must compare the
canonical public credential within the current user and venue/network scope;
it must not select a secret across accounts merely because a label matches.
The same public fields are required on an explicit settings request.

Native TWAP state must retain its identity `ownerId` when an account label changes.
A legacy record without that owner cannot be attributed from its label and is
refused by the existing owner fence; hosts must not automatically replay it.

## Rolling this change

Before this change only Bybit was identity-keyed; every other hosted account's
TWAP rows carry the bare user ID as `ownerId`. This build owns them by the actor
digest, so a row saved by one build is not restored by the other: the owner fence
skips it, not even paused. There is no dual-owner acceptance. Roll this release,
and roll it back, only with zero TWAPs on the box being restarted, using the
pre-roll check in `sdk-rs/hosts/server/DESIGN.md` under "Drain and roll".

Order submit, acknowledgement, outcome and receipt formats are unchanged.
An unknown write outcome is never re-sent by this work. Identity or metadata
refusal does not authorize replay of an earlier write.
