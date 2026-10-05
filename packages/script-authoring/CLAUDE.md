# CLAUDE.md — @tealstreet/script-authoring

Validation core for custom scripts, plus the OpenAI transport used by the
save-time safety screen, the AI order planner and the AI commands. Transport-agnostic on purpose: the MCP server and the
web editor are two frontends over this one package, so a script can never be
"valid" in one surface and invalid in the other.

**We do not generate code for users.** The drafting half of this package
(`generateScript`, `repairScript`, `explainScript`, the prompt pack) was
removed: users bring their own tooling and pay for their own inference, and
Tealstreet hosts an MCP connector rather than an AI service. The only provider
call left in the product is the module safety screen, which is why `openai.ts`
is a bare JSON transport with no opinion about what it is asking for.

Spec: [`docs/custom-scripts/SPEC.md`](../../docs/custom-scripts/SPEC.md).

## Why this is not in `script-api-core`

`typescript` is a **runtime** dependency here (hence `dependencies`, not
`devDependencies`) — the compiler API is the validator. `script-api-core`
ships into the browser worker and must stay portable, so it cannot carry it.

## `validate()` is four passes, and the last three are the interesting ones

```
validate = typecheck  ++  checkRuntimeImports  ++  checkCapabilities  ++  checkTargetSurface
```

**`checkRuntimeImports` covers what the typechecker structurally cannot see.**
The types bundle declares `declare module '@tealstreet/script-api-core'`, so

```ts
import { OrderSide } from '@tealstreet/script-api-core';
```

typechecks clean — and then dies at load, because `scripts-store` loads
scripts with `import()` from `~/.tealstreet/scripts/`, a directory with no
`node_modules`. This is the single most likely thing an AI loop emits, since
it is what every other TypeScript codebase looks like. Static, default,
namespace, side-effect, re-export, and dynamic-import forms are all rejected;
`import type` is fine.

**`checkCapabilities` is the authoring-time sibling of the dispatcher gate**
in `script-api-core/src/runtime/capabilityCheck.ts`. It has a strict posture:
**false negatives are acceptable, false positives are not.** It drives an
automated repair loop, and a spurious finding sends the model in circles
editing a correct declaration. Every ambiguity resolves to silence —
dynamically-built `meta`, non-literal arrays, unreadable `runCommand` blocks,
aliasing, destructuring, element access.

`closePosition` requires **both** `closePosition` and `placeOrder`, mirroring
`buildApi`: the adapters synthesize it as `placeOrder({reduceOnly:true})`.

**`checkTargetSurface` is the authoring-time sibling of the per-runtime API
surface descriptors in `script-api-core/src/runtime/runtimeSurface.ts`.** It
reads `meta.targets` as a support set and verifies every statically-known
`api.*` path is available on every declared runtime. Unlike the capability
check, it chases common aliases/destructuring (`const ex = api.exchange`,
`const { getTicker } = api`) because target violations are deterministic and
need actionable errors before marketplace publish or MCP write.

`validate()` suppresses the AST passes when the source has syntax errors —
speculative findings stacked on a parse failure bury the one thing that
actually needs fixing.

## The screen's rule tables must be looked up as OWN properties

`circumvention.ts` matches paths against three object-literal tables. A bare
`TABLE[path]` is a prototype-chain lookup, so `toString`, `toLocaleString`,
`constructor`, `valueOf` and `hasOwnProperty` all resolve to a truthy function
— and spreading a function into a finding yields one with no `rule`, no
`severity` and no `detail`. **10 of the 70 production modules produced these**,
mostly from ordinary `n.toLocaleString()` number formatting.

Tiering survived it, because `requiredTier` and `hasBlockingFinding` test for a
specific severity and `undefined` is not one. The damage was upstream: the junk
findings are fed to the model by `buildScreenPrompt`, and the screen is
**fail-closed**, so noise that reads as evidence of tampering can only push a
verdict toward refusing a module its author is trying to save. Go through
`ownRule()`.

**And `BLOCKED_PROPERTY_WRITES` is built with `Object.fromEntries`, not a
literal.** In a literal, `__proto__: value` invokes the prototype setter rather
than creating an own property, so an own-property lookup finds nothing and the
prototype-pollution `block` rule silently stops firing. Adding the
`hasOwnProperty` guard broke exactly this, and the "blocks prototype pollution"
test is what caught it — keep that test.

## The compiler host has three load-bearing properties

Each is pinned by a test; breaking any one is silent:

1. **`/types.d.ts` must be a rootName.** Otherwise every script fails TS2307.
2. **The composed bundle must stay a non-module script.** Its `declare module`
   blocks are ambient only while the file has no top-level `import`/`export`.
   If the generator ever emits one they become _augmentations_ and resolution
   collapses.
3. **The host must not expose `node_modules`,** or resolution escapes to the
   real package and validates the wrong surface.

`getSourceFile`'s disk fallback is gated on the TypeScript lib directory.
`fileExists` constrains module resolution but **not** a `/// <reference path>`
directive, which asks the host for an arbitrary path directly — without that
gate, untrusted source could read any file on the box and poison the cache.
This validator accepts untrusted source over MCP and a web route, so that is
a real vulnerability, not tidiness.

The types SourceFile cache is keyed by **content hash**, not filename: two
callers on different bundle versions would otherwise silently share whichever
compiled first.

## `openai.ts` is a transport, not a client for a feature

`OpenAiJsonClient.complete()` takes a schema and returns parsed JSON. It does
not know what it is asking or validate the shape — that belongs to the caller,
so the next provider-backed feature does not have to fork the transport.

Load-bearing details carried over from the drafting client, each of which cost
a debugging session to find:

- **Reasoning tokens count against `max_output_tokens`.** Too small a budget
  truncates silently and surfaces as "did not include output text", a
  confusing failure a long way from its cause. `status === 'incomplete'` is
  checked explicitly so that failure names itself.
- **A refusal is not a parse error.** `getOutputText` throws `AiProviderError`
  on a `refusal` content item; that must be rethrown intact rather than being
  caught by the surrounding JSON `try` and relabelled "malformed JSON". The
  original drafting client had exactly this bug.
- **`prompt_cache_key` must be versioned by the caller.** A prompt change that
  reuses the previous cache key serves the old instructions.
- **`completeWithUsage` reports what a call cost** (`input`, `cached input`
  — a SUBSET of input — and `output`, which already includes reasoning), and
  `AiProviderError.usage` carries it for a call that was billed but still
  failed. The AI credits ledger (`apps/web/src/app/api/ai/credits/`) charges
  from it; `complete()` is unchanged for callers that do not care.
- **`effort: 'none'` omits `reasoning`** — gpt-4.1 and gpt-4o reject the
  parameter. Unset still means `'low'`, which every existing caller relies on.

## No caps here

`config.ts` carries no request limit on purpose. A per-process counter in a
shared library used by a multi-instance web deployment and a local CLI is
neither a real limit nor per-user. Rate limiting belongs to the consumer — for
the screen that is `apps/web/src/app/api/utils/screenAllowance.ts`, backed by
the persisted `script_ai_usage` table.

There is no default model id — `getScreenConfig` throws when
`TEALSTREET_MODULE_SCREEN_MODEL` is unset, so a misconfiguration fails at
startup with a clear message rather than at request time with a provider error.

**The consumer's cap is fail-closed and therefore blocks saves,** not just a
model call. Read `screenAllowance.ts` before changing any limit here — a value
that would merely degrade a generation feature will stop users saving their own
work.

`OPENAI_API_KEY` is read server-side only and must never reach a browser
bundle.

## The script-types bundle lives here, not in `apps/cli`

`script-types-bundle.ts` (generated), its drift test, and `scripts-templates.ts`
moved here from `apps/cli/src/`. Not a tidiness change — `apps/cli` and this
package had become a turbo dependency **cycle** (`cli` depends on this package;
this package devDepended on `cli` for two test imports, and turbo's `^build`
follows devDependencies), and turbo refuses to run _any_ target when the graph
has a cycle. `yarn build` was broken for the whole repo, in CI and on Vercel.

The fix had to be the move, not just dropping the devDependency: `apps/web` also
imported `@tealstreet/cli/src/script-types-bundle`, and with the cycle gone
turbo would have started running the CLI's **bun cross-compile of four platform
binaries** inside every web and CI build. The cycle was hiding that. Moving the
shared constants let `@tealstreet/cli` leave web's dependency graph entirely.

Consequences to preserve:

- **`scripts/generate-script-types-bundle.ts` writes here.** Never hand-edit
  `script-types-bundle.ts`; fix the generators.
- **The drift test is a release gate and must stay wired.** It re-runs the type
  generators and byte-compares, which is what stops a release shipping stale
  `types.d.ts` to users. It runs in `ci.yaml` and `cli-release.yml` explicitly,
  because root `yarn test-ci` filters to `tealstreet-next` and would never
  reach `packages/`.
- **A package must not depend on an app.** If something here needs a value from
  `apps/cli`, that value is in the wrong place.
- **`mirrors/copy.bara.sky` includes `packages/script-authoring/**`**, because
  `apps/cli/src/scripts-store.ts` — an eagerly loaded core module, not the lazy
  MCP path — imports `scripts-templates` from here. Without it the CLI does not
  build in the OSS mirror.
