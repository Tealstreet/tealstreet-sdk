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

- Tealstreet integrator account indexes for mainnet/testnet.
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
retained here. No replacement script is provided.

The recorded probe constructed a `LighterApiClient` with request/response
logging and explored account/configuration reads plus candidate
`ApproveIntegrator` and revocation payloads. Those observations are
historical evidence only; use the official references above and a separately
authorized, purpose-built procedure for any future live investigation.
## SDK implementation checklist (post-discovery)

- Add `ApproveIntegrator` tx support in local `TransactionClient`:
  - params typing
  - hash/sign input ordering
  - validation ranges
- Extend `http-types` with `L2ApproveIntegratorTxInfo` shape.
- Extend `sendRawTx()` mapping to include tx type `45`.
- Add optional `L1Sig` pass-through for cross-master flow.
- Add tests:
  - tx payload serialization
  - validation/revocation rules
  - transport payload mapping for tx type `45`
- Gate production UI/flow behind a feature flag until real integrator indexes are set.

## Current status

- SDK support for `ApproveIntegrator` is implemented in local `@tealstreet/lighter-sdk` (tx type `45`, signing, transport mapping).
- Account-form wiring exists via:
  - `apps/web/src/components/settings/accountsSettings/LighterBuilderFeeApproval.tsx`
  - helper route `apps/web/src/app/[locale]/trade/lighter-l1sig/page.tsx`
