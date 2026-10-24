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

## Implemented checkpoint

uufub4 cleared plan 209c6cfa595 and independently reconstructed exact preview
SHA256 e640befcfcd24ad4c567b4a74c2ab18f8479a62d7ca0cfbab65ca18e50baa8b5.
The applied test is byte-exact to that preview, SHA256
78667013803656fe162ef62ed75f6a3ae4562619bd8557392141fca9228c9244.
Formatting leaves exactly the three approved line changes. Every held-request,
abort, engine and metadata assertion is retained; only the diagnostic's configured
numeric duration changes. Production, other tests, workflows and profiles are
untouched.

The unchanged original native file passed 11/11 locally: a working control,
not a deterministic reproduction of the CI flake. After the correction, the same
retained CI artifact passes 11/11 native tests and the ordinary factory unit
file passes 18/18. Explicit changed-file ESLint and Prettier checks pass, as does
the scoped safe-cex-server TypeScript check. No Cargo or WASM build was run.

The retained artifact is ci-wasm-binance ID 12571423778 from Rust run 40121997239;
its build.json pins producer source 70bf19ff47e3788b5eab85994f84f70dabc3ff2f
and WASM SHA256 ceb24d803b28cc26575dcbed4fbde3997498560106f47b54d1dab6d72bcf3f6f.
This is fixture verification against that exact producer, not a new current-head
compiled-artifact claim. The temporary overlay restored this lane's generated
Node package byte-exact afterward. Fresh PR CI owns the final composed build.
Final committed-source review and the auditor's landing order remain gates.
