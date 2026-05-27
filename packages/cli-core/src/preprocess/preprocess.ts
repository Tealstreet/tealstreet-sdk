import { expandRepeats } from './repeat';
import { splitChain } from './tokenize';
import { preprocessSegment, expandSegmentAlias } from './substitute';
import { tryExpandLoop } from './loop';
import { tryExpandIfElse } from './ifElse';
import type { PreprocessContext, PreprocessResult, PreprocessedStep, PreprocessWarning } from './types';

/** Steps whose head matches this run with `block: true` so the runner
 *  awaits them. `sleep` always blocks (the original pacing primitive);
 *  `wait` blocks because most of its forms are condition-polling and
 *  depend on the runner to await before firing later steps. */
const SLEEP_HEAD_RE = /^(sleep|wait)(\s|$)/;
const LEGACY_AT_PREFIX_RE = /^@(\S+)/;
/** `set ec` (no value) flips a chain-level continue-on-error flag. */
const SET_EC_RE = /^set\s+ec\s*$/i;
/** `set ro` / `unset ro` — chain-wide reduce-only flag. Subsequent
 *  order-placing steps get a `reduce` keyword injected. */
const SET_RO_RE = /^set\s+ro\s*$/i;
const UNSET_RO_RE = /^unset\s+ro\s*$/i;
/** `set po` / `unset po` — chain-wide post-only flag. Subsequent buy/
 *  sell steps get a `po` keyword injected. Market orders ignore it. */
const SET_PO_RE = /^set\s+po\s*$/i;
const UNSET_PO_RE = /^unset\s+po\s*$/i;
/** `retry N <body>` — N must be 0..10. Anything else falls through. */
const RETRY_PREFIX_RE = /^retry\s+(\d+)\s+(\S[\s\S]*)$/i;
const RETRY_MAX = 10;
/** Bare `end` (case-insensitive, no args) — early-terminate the chain.
 *  Subsequent segments are dropped. Useful from inside an `if` branch
 *  to short-circuit the rest of the chain when a condition holds. */
const END_RE = /^end\s*$/i;
/** `@<name> = <body>` — capture the body's result.data.orderIds into
 *  the named user var after the step runs. The runner will await the
 *  step so it can read the IDs before continuing the chain. */
const CAPTURE_PREFIX_RE = /^@([a-zA-Z][\w]*)\s*=\s*(\S[\s\S]*)$/;

/** Command heads where injecting `reduce` makes sense. Heads outside
 *  this set are left untouched. Allowed: buy/sell (+ aliases),
 *  scale, swarm, chase, twap. close/stop are already reduce-only by
 *  construction so injecting is redundant; we still leave them out so
 *  the emitted command stays clean. */
const RO_INJECT_HEADS = new Set(['buy', 'b', 'sell', 's', 'scale', 'swarm', 'chase', 'twap']);
/** Command heads where injecting `po` makes sense. Only buy/sell
 *  honour the `po` keyword (limit orders). On market orders the
 *  keyword is ignored. */
const PO_INJECT_HEADS = new Set(['buy', 'b', 'sell', 's']);

/**
 * Preprocess a raw command string into the executable step stream the
 * runner walks. Pipeline:
 *
 *   1. Expand `repeat N (...)` blocks (recursive).
 *   2. Split the result on top-level `;` and `&` (paren- and quote-aware).
 *   3. For each segment:
 *      - Detect the legacy `@cmd` parallel prefix and strip it (one-time
 *        warning per call). Skipped if `@cmd` matches a known user var.
 *      - Expand the leading alias (without substitution) so that
 *        multi-statement alias bodies splice back into the chain. The
 *        result is re-split with `splitChain` and each piece is then
 *        run through `preprocessSegment` for substitution. Special-cased
 *        commands (`alias`/`unalias`/`print`/`unset` and `set @x …`)
 *        bypass the alias expansion so a quoted body with embedded
 *        `;` stays opaque.
 *      - Flag `sleep ...` segments with `block: true` so the runner
 *        awaits them instead of fire-and-forget.
 *      - Flag the first step of a `;`-separated segment with
 *        `awaitPrior: true` so the runner drains in-flight steps before
 *        it (a barrier); `&`-separated steps stay parallel.
 *
 * Chain semantics the runner MUST honor: `&` (and the chain head) are
 * fire-and-don't-await; a step with `awaitPrior` is a barrier — the
 * runner drains all in-flight fire-and-forget steps before dispatching
 * it, so `close all; buy $1000` waits for the close. `block` steps
 * (`sleep`/`wait`), `retry`, and `captureTo` steps are awaited inline.
 */
export function preprocessCommand(
  input: string,
  ctx: PreprocessContext,
  // Pending captures inherited from an OUTER chain. Set by the
  // if-expansion recursion path so a branch body's deferred-
  // substitution sees the outer chain's pending captures too.
  // Top-level callers omit this — a fresh chain starts empty.
  outerPendingCaptures?: ReadonlySet<string>,
): PreprocessResult {
  const warnings: PreprocessWarning[] = [];
  const seenWarningCodes = new Set<PreprocessWarning['code']>();

  const pushWarning = (w: PreprocessWarning) => {
    if (seenWarningCodes.has(w.code)) return;
    seenWarningCodes.add(w.code);
    warnings.push(w);
  };

  const expanded = expandRepeats(input);
  const segments = splitChain(expanded);

  const steps: PreprocessedStep[] = [];

  // Chain-level modifiers — flipped on by `set ec` / `set ro` /
  // `set po` and applied to subsequent steps in this preprocess
  // invocation. Per-chain only; a fresh `preprocessCommand` call
  // starts with all flags off.
  let continueOnError = false;
  let reduceOnly = false;
  let postOnly = false;
  // `end` early-terminate — set when a bare `end` segment is hit or
  // when a nested `preprocessCommand` (from `if` recursion) returns
  // with `endEarly: true`. The outer loop checks this at the top of
  // each iteration so subsequent segments don't emit.
  let endEarly = false;
  // Pending captures — `@<name>` tokens that THIS chain's earlier
  // steps will `captureTo`. Later segments referencing these names
  // leave them literal in `command` (via `deferredSubstitution`) and
  // the runner finalises after the prior step's writeback. This is
  // what makes single-prompt `@o1 = buy …; cancel @o1` work.
  //
  // Seeded from the outer chain (when this preprocessCommand call is
  // running inside an if-branch recursion) so the inner body
  // inherits the outer chain's pending captures.
  const pendingCaptures = new Set<string>(outerPendingCaptures ?? []);

  for (const seg of segments) {
    if (endEarly) break;
    let raw = seg.command;

    // `;` vs `&`: the operator preceding a segment decides whether the
    // runner drains in-flight fire-and-forget steps before this one (a
    // barrier). `;` = sequential (await prior), `&` = parallel. Only the
    // FIRST step a segment emits carries the barrier vs the previous
    // segment; interior steps of an expansion (loop/if fan-out) stay
    // parallel among themselves unless their source used `;`.
    const segAwait = seg.separator === ';';
    let segFirst = true;

    // `set ec` / `set ro` / `set po` — chain modifiers, not steps.
    // Flip the flag and skip emitting anything for this segment.
    // `unset ro` / `unset po` reverse them mid-chain so the user can
    // toggle ro/po per-order-group without leaving the chain.
    const trimmedRaw = raw.trim();
    if (SET_EC_RE.test(trimmedRaw)) {
      continueOnError = true;
      continue;
    }
    if (SET_RO_RE.test(trimmedRaw)) {
      reduceOnly = true;
      continue;
    }
    if (SET_PO_RE.test(trimmedRaw)) {
      postOnly = true;
      continue;
    }
    if (UNSET_RO_RE.test(trimmedRaw)) {
      reduceOnly = false;
      continue;
    }
    if (UNSET_PO_RE.test(trimmedRaw)) {
      postOnly = false;
      continue;
    }

    // `retry N <body>` — apply retry to this single segment, then
    // strip the prefix so substitution sees only the body.
    let retryCount: number | undefined;
    const retryMatch = raw.match(RETRY_PREFIX_RE);
    if (retryMatch) {
      const n = parseInt(retryMatch[1], 10);
      if (Number.isFinite(n) && n >= 0 && n <= RETRY_MAX) {
        retryCount = n;
        raw = retryMatch[2];
      }
      // Out-of-range retry counts fall through — the segment runs as
      // a regular step. The CLI's command dispatcher will then see
      // a literal `retry 99 buy 1$` and reject it.
    }

    // `@<name> = <body>` capture syntax — bind the body's
    // result.data.orderIds into the named user var. Detected BEFORE
    // the legacy `@cmd` strip below (an `@x = …` form must not be
    // mistaken for a legacy parallel prefix and the `@x` stripped).
    // captureTo is then attached to every step the body produces.
    // If the body alias-expands to multiple sub-segments, each step
    // writes to the same var (last write wins) — documented limitation
    // in commands.md.
    let captureTo: string | undefined;
    const captureMatch = raw.match(CAPTURE_PREFIX_RE);
    if (captureMatch) {
      captureTo = captureMatch[1];
      raw = captureMatch[2];
    }

    // Legacy `@cmd` parallel prefix → strip + warn, unless `@<word>` is
    // a known user var (in which case substitution handles it normally).
    // Skipped when we already peeled a capture prefix — the body is
    // free of leading `@`.
    if (captureTo === undefined) {
      const legacy = raw.match(LEGACY_AT_PREFIX_RE);
      if (legacy && ctx.store.getVar(legacy[1]) === undefined) {
        pushWarning({
          code: 'deprecated-at-prefix',
          message: "the '@cmd' parallel prefix is deprecated — use '&' between commands instead",
        });
        raw = raw.slice(1);
      }
    }

    // Alias-expand the head of this segment up front, then re-split the
    // result. This lets `alias pair "buy 1$; sell 1$"` produce two
    // chain steps when the user types `pair`. Non-alias segments and
    // FULL_PASSTHROUGH heads pass through unchanged here.
    const aliasExpanded = expandSegmentAlias(raw, ctx);
    const subSegments: Array<{ command: string; separator?: ';' | '&' | null }> =
      aliasExpanded === raw ? [{ command: raw, separator: null }] : splitChain(aliasExpanded);

    for (const sub of subSegments) {
      if (endEarly) break;

      // Per emitted step: the segment's first step carries the
      // segment-level `;`/`&` barrier; a later alias sub-statement's
      // first step carries its own separator; interior expansion steps
      // (loop/if fan-out) stay parallel.
      let subFirst = true;
      const emitAwait = (): boolean => {
        let a: boolean;
        if (segFirst) a = segAwait;
        else if (subFirst) a = sub.separator === ';';
        else a = false;
        segFirst = false;
        subFirst = false;
        return a;
      };

      // Bare `end` — early-terminate the entire chain. Sits AFTER
      // alias expansion so `alias bail end` then `bail; buy 1$` cuts
      // at the alias-expanded `end`. Sits before `if` / `loop` /
      // substitution so `end` itself never reaches the dispatcher.
      // Any `retry N` prefix peeled above is silently discarded —
      // retry on a chain-terminator is meaningless.
      if (END_RE.test(sub.command.trim())) {
        endEarly = true;
        break;
      }

      // `if (<cond>) (<then>) [else (<else>)]` — evaluate the
      // condition once against current store state and splice the
      // chosen branch's body back into the chain. We recurse into
      // `preprocessCommand` so the branch body is itself fully
      // processed (nested ifs, repeats, loops, set ec, retry —
      // everything composes). Outer chain-modifier flags
      // (retry / errorMode) propagate to inner steps that haven't
      // set their own.
      const ifResult = tryExpandIfElse(sub.command, ctx);
      if (ifResult) {
        const body = ifResult.body.trim();
        if (body !== '') {
          // Pass outer pendingCaptures so the inner body's
          // deferred-substitution sees the outer chain's pending
          // captures too — e.g. `@o1 = buy; if (cond) (cancel @o1)`
          // resolves the inner @o1 reference correctly.
          const inner = preprocessCommand(body, ctx, pendingCaptures);
          for (const w of inner.warnings) pushWarning(w);
          let ifFirst = true;
          for (const innerStep of inner.steps) {
            // The inner step already had any inner-branch `set ro/po`
            // applied (it went through preprocessCommand). For OUTER
            // ro/po flags we inject here. The naive injection isn't
            // idempotent across nested branches, but
            // applyChainOrderModifiers's hasReduce / hasPo check
            // covers the same-command-already-has-keyword case.
            const finalCommand = applyChainOrderModifiers(innerStep.command, reduceOnly, postOnly);
            const merged: PreprocessedStep = {
              command: finalCommand,
              block: innerStep.block,
            };
            const inheritedRetry = innerStep.retry ?? retryCount;
            if (inheritedRetry !== undefined) merged.retry = inheritedRetry;
            const inheritedEC = innerStep.errorMode ?? (continueOnError ? 'continue' : undefined);
            if (inheritedEC !== undefined) merged.errorMode = inheritedEC;
            // captureTo on the OUTER segment applies to every step
            // emitted from the if's chosen branch — so `@x = if (cond)
            // (buy 1$)` correctly tags the buy. An inner branch can't
            // set its own captureTo today (no `@x = if …` shorthand
            // inside branches), so this is one-way inheritance.
            if (captureTo !== undefined) merged.captureTo = captureTo;
            // Carry through the inner step's deferred substitutions —
            // the inner body saw the outer chain's pending captures
            // (passed via the recursive preprocessCommand call above).
            if (innerStep.deferredSubstitution) {
              merged.deferredSubstitution = innerStep.deferredSubstitution;
            }
            // The if-branch's FIRST step carries the outer segment's
            // barrier; later inner steps keep their own `;`/`&` (computed
            // by the recursive preprocessCommand for the branch body).
            const aw = ifFirst ? emitAwait() : innerStep.awaitPrior;
            ifFirst = false;
            if (aw) merged.awaitPrior = true;
            steps.push(merged);
            if (merged.captureTo !== undefined) {
              pendingCaptures.add(merged.captureTo);
            }
          }
          // Propagate `end` upward — an `end` inside the chosen branch
          // terminates the OUTER chain too, not just the branch body.
          if (inner.endEarly) {
            endEarly = true;
            break;
          }
        }
        continue;
      }

      // `loop <variant>: <body>` — expand to one step per matching
      // position. Runs after alias expansion so `alias l3 'loop
      // longs: buy 1$'` then `l3` works.
      const loop = tryExpandLoop(sub.command, ctx);
      if (loop) {
        for (const expanded of loop.expansions) {
          const deferred = new Set<string>();
          const command = preprocessSegment(expanded, ctx, pendingCaptures, deferred);
          if (!command) continue;
          const step = makeStep(
            command,
            retryCount,
            continueOnError,
            reduceOnly,
            postOnly,
            captureTo,
            deferred,
            emitAwait(),
          );
          steps.push(step);
          if (step.captureTo !== undefined) pendingCaptures.add(step.captureTo);
        }
        continue;
      }

      const deferred = new Set<string>();
      const command = preprocessSegment(sub.command, ctx, pendingCaptures, deferred);
      if (!command) continue;
      const step = makeStep(
        command,
        retryCount,
        continueOnError,
        reduceOnly,
        postOnly,
        captureTo,
        deferred,
        emitAwait(),
      );
      steps.push(step);
      if (step.captureTo !== undefined) pendingCaptures.add(step.captureTo);
    }
  }

  const result: PreprocessResult = { steps, warnings };
  if (endEarly) result.endEarly = true;
  return result;
}

function makeStep(
  command: string,
  retry: number | undefined,
  continueOnError: boolean,
  reduceOnly = false,
  postOnly = false,
  captureTo?: string,
  deferredCaptures?: Set<string>,
  awaitPrior = false,
): PreprocessedStep {
  const finalCommand = applyChainOrderModifiers(command, reduceOnly, postOnly);
  const step: PreprocessedStep = {
    command: finalCommand,
    block: SLEEP_HEAD_RE.test(finalCommand),
  };
  if (retry !== undefined) step.retry = retry;
  if (continueOnError) step.errorMode = 'continue';
  if (captureTo !== undefined) step.captureTo = captureTo;
  // Sorted for deterministic ordering — tests compare arrays.
  if (deferredCaptures && deferredCaptures.size > 0) {
    step.deferredSubstitution = [...deferredCaptures].sort();
  }
  if (awaitPrior) step.awaitPrior = true;
  return step;
}

/** Inject `reduce` and/or `po` keywords into the command string when
 *  the relevant chain flag is set AND the head is an order-placing
 *  command. Idempotent — won't double-inject if the user already
 *  wrote `reduce` or `po` in the command. */
function applyChainOrderModifiers(command: string, reduceOnly: boolean, postOnly: boolean): string {
  if (!reduceOnly && !postOnly) return command;
  const head = command.split(/\s+/)[0]?.toLowerCase();
  if (!head) return command;
  const wantsRO = reduceOnly && RO_INJECT_HEADS.has(head);
  const wantsPO = postOnly && PO_INJECT_HEADS.has(head);
  if (!wantsRO && !wantsPO) return command;
  // Naive whitespace tokens — fine for our trading-command surface
  // which doesn't quote args after preprocessing.
  const tokens = command.split(/\s+/);
  const hasReduce = tokens.some((t) => t.toLowerCase() === 'reduce');
  const hasPo = tokens.some((t) => t.toLowerCase() === 'po');
  const append: string[] = [];
  if (wantsRO && !hasReduce) append.push('reduce');
  if (wantsPO && !hasPo) append.push('po');
  return append.length > 0 ? `${command} ${append.join(' ')}` : command;
}
