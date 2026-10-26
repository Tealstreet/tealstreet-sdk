# Lighter ApproveIntegrator Discovery Runbook

This runbook is for discovery and data collection before implementing production approval UX.

## Source of truth references

- [Lighter Partner Integration](https://apidocs.lighter.xyz/docs/partner-integration)
- [Lighter Go SDK](https://github.com/elliottech/lighter-go)
- [Lighter Python SDK](https://github.com/elliottech/lighter-python)
- Secondary reference only: [lighter-rs crate page](https://lib.rs/crates/lighter-rs)

## Confirmed transaction contract

The following `ApproveIntegrator` details are confirmed from official references:

- `tx_type = 45`
- `tx_info` fields (PascalCase):
  - `AccountIndex`
  - `ApiKeyIndex`
  - `IntegratorAccountIndex`
  - `MaxPerpsTakerFee`
  - `MaxPerpsMakerFee`
  - `MaxSpotTakerFee`
  - `MaxSpotMakerFee`
  - `ApprovalExpiry`
  - `ExpiredAt`
  - `Nonce`
  - `Sig`
  - optional `L1Sig` for cross-master approval path
- Revocation shape:
  - all max-fee fields = `0`
  - `ApprovalExpiry = 0`

## Information still needed

- Tealstreet integrator account index for testnet.
- Robinhood Chain builder attribution uses Robinhood-specific account index
  `47296`; do not reuse the ordinary zkLighter mainnet index for Robinhood.
- First required production flow:
  - same-master (`L2` signature only), or
  - cross-master (`L1Sig` required).
- Approved-integrator read endpoint contract to drive UI state (`approved` vs `not approved`).
- Error semantics from `sendTx` responses for:
  - bad signature
  - invalid integrator index
  - expired approval
  - fee cap violations

## Historical discovery-script record

The deleted `test-approve-integrator-discovery.ts` was an untrusted,
one-off probe used to collect the historical findings above. Its environment
variables, live submission scenarios, and run command are intentionally not
retained here. Do not restore a probe that signs or submits approval
transactions without a fresh, authorized validation plan.

For Robinhood builder account-index discovery only, use the read-only lookup
probe:

```bash
yarn probe:lighter-rh-builder-index --l1 0x...
```

That probe only queries Lighter account lookup endpoints and prints candidate
`accountIndex` values. It does not sign, call `sendTx`, approve an integrator,
or validate attributed order placement.

The recorded probe constructed a `LighterApiClient` with request/response
logging and explored account/configuration reads plus candidate
`ApproveIntegrator` and revocation payloads. Those observations are
historical evidence only; use the official references above and a separately
authorized, purpose-built procedure for any future live investigation.

## Implemented SDK support

Local `@tealstreet/lighter-sdk` now has `ApproveIntegrator` support:

- `TransactionClient.getApproveIntegratorTransaction()` signs tx type `45`.
- `http-types` includes the `L2ApproveIntegratorTxInfo` shape.
- `sendRawTx()` maps approval payloads through the tx type `45` transport path.
- Optional `L1Sig` pass-through exists for cross-master approval contexts.
- Tests cover tx payload serialization, validation/revocation rules, and
  transport payload mapping.

Keep future SDK work focused on contract drift or new upstream requirements,
not re-adding the already implemented tx type.

## Current status

- SDK support for `ApproveIntegrator` is implemented in local `@tealstreet/lighter-sdk` (tx type `45`, signing, transport mapping).
- Account-form wiring exists via:
  - `apps/web/src/components/settings/accountsSettings/LighterBuilderFeeApproval.tsx`
  - helper route `apps/web/src/app/[locale]/(bare)/trade/lighter-l1sig/page.tsx`
- Ordinary zkLighter mainnet has a configured default integrator account index.
- Robinhood Chain Lighter has a configured default integrator account index:
  `47296`.
- Robinhood `ApproveIntegrator` and attributed order placement still need live
  validation before production rollout.
