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

## Discovery script

Script path:

- `packages/safe-cex/src/exchanges/lighter/test-approve-integrator-discovery.ts`

What it does:

- Creates a `LighterApiClient` probe instance with request/response/error logging.
- Adds `X-Timestamp` and `X-Account-Index` headers (like runtime exchange wiring).
- Probes likely read endpoints (`/api/v1/account`, `systemConfig` variants).
- Sends multiple `ApproveIntegrator` candidate payloads to `/api/v1/sendTx`:
  - canonical PascalCase payload
  - revocation payload
  - optional malformed snake_case payload
- Supports two execution modes:
  - placeholder signature mode (`LIGHTER_USE_REAL_SIGNER=false`)
  - real-signature mode (`LIGHTER_USE_REAL_SIGNER=true`) using local API private key
- Prints structured summary output for artifact capture.

## Environment config

Export these variables before running:

```bash
LIGHTER_ACCOUNT_INDEX=123456
LIGHTER_API_KEY_INDEX=0
LIGHTER_API_URL=https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://mainnet.zklighter.elliot.ai

# Optional, defaults shown
LIGHTER_INTEGRATOR_ACCOUNT_INDEX=0
LIGHTER_MAX_PERPS_TAKER_FEE=300
LIGHTER_MAX_PERPS_MAKER_FEE=300
LIGHTER_MAX_SPOT_TAKER_FEE=300
LIGHTER_MAX_SPOT_MAKER_FEE=300
# omit to auto-fill defaults:
# LIGHTER_APPROVAL_EXPIRY_MS=
# LIGHTER_TX_EXPIRED_AT_MS=
LIGHTER_APPROVE_SIG_B64=AQ==
LIGHTER_APPROVE_L1_SIG=
LIGHTER_APPROVE_L1_SIG_REVOCATION=
LIGHTER_NONCE=
LIGHTER_TESTNET=false
LIGHTER_SUBMIT_DISCOVERY_REQUESTS=true
LIGHTER_USE_REAL_SIGNER=false
LIGHTER_API_PRIVATE_KEY=
LIGHTER_RUN_REVOCATION_SCENARIO=true
LIGHTER_PROBE_READ_ENDPOINTS=true
LIGHTER_INCLUDE_MALFORMED_SCENARIO=true
LIGHTER_REQUEST_TIMEOUT_MS=20000
LIGHTER_PRICE_PROTECTION=false
```

Notes:

- If `LIGHTER_APPROVAL_EXPIRY_MS` or `LIGHTER_TX_EXPIRED_AT_MS` are omitted, the script auto-fills sensible defaults.
- Without valid signatures, failure responses are expected and still useful.
- For end-to-end approval submission testing, set:
  - `LIGHTER_USE_REAL_SIGNER=true`
  - `LIGHTER_API_PRIVATE_KEY=<api private key for LIGHTER_ACCOUNT_INDEX/api key index pair>`
- `L1Sig` is payload-specific. If revocation is enabled in signer mode, provide a separate revocation signature:
  - `LIGHTER_APPROVE_L1_SIG` for canonical approval payload
  - `LIGHTER_APPROVE_L1_SIG_REVOCATION` for revocation payload
  - or set `LIGHTER_RUN_REVOCATION_SCENARIO=false` for approval-only runs

### Browser wallet helper for `L1Sig`

If `sendTx` returns `21504 fail to l1 signature`, generate `L1Sig` with wallet popup using:

- Route: `apps/web` -> `/trade/lighter-l1sig`
- File: `apps/web/src/app/[locale]/trade/lighter-l1sig/page.tsx`

Suggested URL query parameters:

```text
/trade/lighter-l1sig?nonce=919&accountIndex=718895&apiKeyIndex=2&integratorAccountIndex=158385&maxPerpsTakerFee=300&maxPerpsMakerFee=300&maxSpotTakerFee=300&maxSpotMakerFee=300&approvalExpiry=1777587105266&testnet=false&wallet=0xYourL1Wallet
```

After signing:

- copy generated signature into `LIGHTER_APPROVE_L1_SIG`
- set `LIGHTER_NONCE` to the same nonce used in the helper page
- rerun the probe script (with `LIGHTER_USE_REAL_SIGNER=true`)

## Run command

From repo root:

```bash
npx tsx packages/safe-cex/src/exchanges/lighter/test-approve-integrator-discovery.ts
```

## Scenario matrix

- `approve-integrator-pascal-case`
  - Goal: test likely-correct wire contract.
  - Expected: either accepted tx hash or deterministic signature/authorization error.
- `approve-integrator-revocation`
  - Goal: validate revoke semantics (`fees=0`, `approvalExpiry=0`).
  - Expected: accepted revoke tx or deterministic validation error.
- `approve-integrator-malformed-snake-case`
  - Goal: force schema mismatch and capture parser error shape.
  - Expected: clear malformed payload rejection.

## Personal test mode (self-owned accounts)

This is a good temporary path before corporate integrator indexes are available.

Recommended setup is two accounts you control:

- Account A = trader account (runs approval + order flow)
- Account B = integrator account (used as `IntegratorAccountIndex`)

Two-account checklist:

1. Create/fund both accounts with minimal safe balances.
2. Configure the probe with Account A credentials and set `LIGHTER_INTEGRATOR_ACCOUNT_INDEX` to Account B index.
3. Run approval scenario (`approve-integrator-pascal-case`) and capture response.
4. Run revocation scenario and capture response.
5. Run an integrator-tagged order flow (tiny size) to capture post-approval behavior.
6. Keep artifacts (request payloads + response bodies + parsed errors) for later SDK wiring.

Example env mapping (script still uses the existing variable names):

```bash
# Trader account (Account A) values:
LIGHTER_ACCOUNT_INDEX=<account_a_index>
LIGHTER_API_KEY_INDEX=<account_a_api_key_index>

# Integrator target (Account B) value:
LIGHTER_INTEGRATOR_ACCOUNT_INDEX=<account_b_index>
```

Single-account fallback is still useful for transport/serialization checks, but it is weaker for validating real attribution economics.

When corporate values are ready, replace only `LIGHTER_INTEGRATOR_ACCOUNT_INDEX` (and any fee-policy values if needed), then rerun the same scenarios.

## Artifact capture template

For each scenario, copy:

- request: `tx_type`, `tx_info`, URL, headers (redacted)
- response: status code + body
- parsed error tuple: `{ code, message, statusCode }`

Suggested JSON record:

```json
{
  "scenario": "approve-integrator-pascal-case",
  "request": {
    "url": "/api/v1/sendTx",
    "tx_type": 45,
    "tx_info": {}
  },
  "response": {
    "status": 400,
    "body": {}
  },
  "parsedError": {
    "code": "SEND_TX_FAILED",
    "message": "example message"
  }
}
```

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
