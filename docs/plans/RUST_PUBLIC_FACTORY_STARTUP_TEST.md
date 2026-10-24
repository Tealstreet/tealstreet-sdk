# PublicExchangeFactory native startup timeout fixture

Owner: codex-hcmq3f. Reviewer: codex-uufub4. Scope: one native WASM test,
its plan and the plan index; no production, workflow, budget or profile change.
Base: actual landing 1ea2c47ed0eb03e8f760b4a30c280c47d4f0d88f.

## Observed failure and cause

Coinbase #2036 Rust run 40122376456 fails in Binance measurement job
120956522259, before CPU measurement. The native server adapter suite reports
10 passing tests and one failure in `bounds a genuinely held native metadata
request and preserves the working engine`: startup times out after 1000 ms at
PublicExchangeFactory.ts:208. The ensuing missing harness/out/ci is a cascade.
The overseer identifies this as a recurring setup failure and assigns this
separate test-only repair. Existing CI evidence is the original failure;
a local passing rerun will not be represented as a deterministic reproduction.

The test creates one factory with nativeStartupTimeoutMs: 1000, then first
awaits a healthy real native startup. Only after that succeeds does it set
`held = true` and force a replacement whose metadata request stays unresolved
until its AbortSignal fires. Thus its short timeout unintentionally applies to
the healthy WASM bootstrap as well as to the deliberately blocked replacement.
Production keeps its existing configurable deadline and 30-second default.

## Proposed correction

Give this single integration fixture a 10,000 ms native startup deadline and
assert its exact corresponding timeout diagnostic. Set a 30,000 ms Vitest
case timeout for this case only, since the blocked operation must reach the
real deadline and cleanup; leave the suite defaults and every other test alone.
This is a test-side bound, not a sleep or a production timeout increase.

Keep the genuine never-resolving metadata transport and all behavioral
assertions: one abort, the same previous engine, previous engine still alive,
identical markets, and factory.get returning the original handle after the
failed replacement. The existing healthy bootstrap still must complete before
`held` is set. No readiness polling, retry, synthetic success or loose error
matcher is added. The real production timeout/disposal path remains exercised;
no production accessor, timer injection or private-field override is introduced.
The only changed assertion is the diagnostic's configured numeric duration.

## Review and verification gates

1. Plan review by uufub4 before changing the test.
2. Preserve the production PublicExchangeFactory.ts blob and every other
   runtime/workflow/profile/budget blob byte-exact to the actual base.
3. Reuse a retained CI Binance WASM artifact with its exact producer pin and
   hash; run the scoped server-adapter native WASM file under the package's
   existing Vitest configuration. Do not claim a new local WASM build.
4. Run the scoped ordinary PublicExchangeFactory unit file and explicit lint
   and formatting checks for the changed test. All native working assertions
   remain in place. Record an unchanged-source local pass honestly if it occurs.
5. Send the exact small test delta and immutable preservation receipt to the
   same reviewer; push only a frozen candidate as a separate draft PR, with
   its one PR-triggered Rust run. Follow the auditor's landing slot ordering.
