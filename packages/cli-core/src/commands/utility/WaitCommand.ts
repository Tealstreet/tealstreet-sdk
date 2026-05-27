import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { parseDuration } from '../../preprocess/sleep';
import { parseConditionText } from '../../preprocess/conditions';

const POLL_INTERVAL_MS = 100;
const DEFAULT_TIMEOUT_MS = 30_000;
const MAX_TIMEOUT_MS = 10 * 60_000;

/**
 * `wait` — block the chain until a condition holds, with timeout.
 *
 * Forms:
 *   wait <duration>                       — same as sleep
 *   wait position [SYMBOL]                — any non-zero position exists
 *   wait long [SYMBOL] / wait short …     — side-specific
 *   wait price (>|<|>=|<=) <number> [SYM] — compare last trade price
 *   wait fill [SYMBOL]                    — every open order on the
 *                                           symbol is gone (filled
 *                                           OR cancelled — the wait
 *                                           doesn't distinguish)
 *
 * Optional `timeout=<duration>` anywhere in the args overrides the
 * default 30s timeout. Hard-capped at 10 minutes — same cap as
 * `sleep`. On timeout the command errors loudly so a chain without
 * `set ec` halts. Combine `set ec; wait …` to make a timeout a soft
 * warning that lets subsequent steps fire.
 *
 * Polls `ctx.exchange.store` every 100ms — same data source the rest
 * of the CLI reads from. This is intentionally not subscription-based
 * so a missed websocket frame can't deadlock the wait beyond its
 * timeout window.
 */
export class WaitCommand extends BaseCommand {
  readonly name = 'wait';
  readonly description = 'Block the chain until a condition holds (with timeout)';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'wait 5s',
    'wait position',
    'wait anyposition',
    'wait long ETHUSDT',
    'wait short',
    'wait price > 50000',
    'wait price < 49500 timeout=10s',
    'wait fill',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const args = ctx.args._.slice(1);
    if (args.length === 0) {
      this.printError(ctx, 'Usage: wait <duration|position|long|short|price <op> <n>|fill> [timeout=<duration>]');
      return this.error('Missing condition');
    }

    // Pull `timeout=<dur>` and `for=<dur>` out anywhere in args.
    //   timeout=<dur> overrides the default 30s timeout.
    //   for=<dur> requires the condition to hold *continuously* for
    //   the given duration before releasing (anti-fakeout). When set,
    //   a single false poll resets the sustain timer.
    let timeoutMs = DEFAULT_TIMEOUT_MS;
    let sustainMs = 0;
    const remaining: string[] = [];
    for (const a of args) {
      const tm = a.match(/^timeout=(.+)$/i);
      if (tm) {
        try {
          timeoutMs = parseDuration(tm[1]);
        } catch (e) {
          this.printError(ctx, `wait: invalid timeout '${tm[1]}': ${(e as Error).message}`);
          return this.error('Invalid timeout');
        }
        if (timeoutMs <= 0 || timeoutMs > MAX_TIMEOUT_MS) {
          this.printError(ctx, `wait: timeout must be in (0, ${MAX_TIMEOUT_MS / 60_000}m]`);
          return this.error('Invalid timeout');
        }
        continue;
      }
      const fm = a.match(/^for=(.+)$/i);
      if (fm) {
        try {
          sustainMs = parseDuration(fm[1]);
        } catch (e) {
          this.printError(ctx, `wait: invalid for '${fm[1]}': ${(e as Error).message}`);
          return this.error('Invalid for');
        }
        if (sustainMs <= 0 || sustainMs > MAX_TIMEOUT_MS) {
          this.printError(ctx, `wait: for must be in (0, ${MAX_TIMEOUT_MS / 60_000}m]`);
          return this.error('Invalid for');
        }
        continue;
      }
      remaining.push(a);
    }

    if (remaining.length === 0) {
      this.printError(ctx, 'Usage: wait <condition> [timeout=<duration>]');
      return this.error('Missing condition');
    }

    // Bare duration — `wait 5s` or `wait 250ms`. Treat as sleep.
    const head = remaining[0].toLowerCase();
    if (this.looksLikeDuration(head) && remaining.length === 1) {
      let durMs: number;
      try {
        durMs = parseDuration(remaining[0]);
      } catch (e) {
        this.printError(ctx, (e as Error).message);
        return this.error((e as Error).message);
      }
      await new Promise<void>((resolve) => setTimeout(resolve, durMs));
      return this.success();
    }

    // `wait time <HHMM>` — wait until the next occurrence of HHMM UTC.
    // If we're already past today's HHMM, waits until tomorrow's.
    if (head === 'time') {
      const t = remaining[1];
      const durMs = this.parseUtcClockTime(t);
      if (durMs === null) {
        this.printError(
          ctx,
          `wait time: expected HHMM (0000-2359) UTC, got '${t}'`,
        );
        return this.error('Invalid time');
      }
      await new Promise<void>((resolve) => setTimeout(resolve, durMs));
      return this.success();
    }

    // From here on we need an exchange to poll against.
    if (!ctx.exchange) {
      return this.error('wait: not connected to an exchange');
    }

    // Re-join remaining args into a single string so the parser can
    // handle paren-grouped combinators (`either (A) or (B)`,
    // `both (A) and (B)`). The simple-form fall-through is identical
    // to passing the tokenised args.
    const condition = parseConditionText(remaining.join(' '));
    if (!condition) {
      this.printError(ctx, `wait: unrecognized condition '${remaining.join(' ')}'`);
      return this.error('Invalid condition');
    }

    const exchange = ctx.exchange!;
    const focusedSymbol = ctx.symbol;
    // The check context carries the PreprocessStore so `@<liveVar>`
    // LHS conditions can resolve at poll time. Other condition kinds
    // ignore it.
    const checkCtx = ctx.preprocessStore ? { store: ctx.preprocessStore } : undefined;
    const check = () => condition.check(exchange, focusedSymbol, checkCtx);
    const deadline = Date.now() + timeoutMs;

    if (sustainMs === 0) {
      // Default: release on first satisfying poll.
      if (check()) return this.success();
      while (Date.now() < deadline) {
        await new Promise<void>((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
        if (check()) return this.success();
      }
      return this.error(`wait timed out (${timeoutMs}ms): ${condition.describe}`);
    }

    // Sustained: the condition must hold continuously for `sustainMs`.
    // Track the timestamp it first became true; a false poll resets.
    let sustainStart: number | null = check() ? Date.now() : null;
    if (sustainStart !== null && Date.now() - sustainStart >= sustainMs) {
      return this.success();
    }
    while (Date.now() < deadline) {
      await new Promise<void>((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
      if (check()) {
        if (sustainStart === null) sustainStart = Date.now();
        if (Date.now() - sustainStart >= sustainMs) return this.success();
      } else {
        sustainStart = null;
      }
    }
    return this.error(
      `wait timed out (${timeoutMs}ms): ${condition.describe} (sustain ${sustainMs}ms)`,
    );
  }

  private looksLikeDuration(s: string): boolean {
    return /^\d+(\.\d+)?(ms|s|m)?$/.test(s);
  }

  /**
   * Parse a 4-digit HHMM UTC time and return the ms-until-next
   * occurrence. Returns `null` on a malformed input. If the time is
   * already in the past today, schedules for tomorrow — the user
   * typed it intentionally and probably wants the next firing, not
   * an immediate no-op.
   *
   * Matches ichibot's `wait time HHMM` semantics, including the
   * "exactly now → wait ~24h" rule.
   */
  private parseUtcClockTime(input: string | undefined): number | null {
    if (!input || !/^\d{4}$/.test(input)) return null;
    const hh = parseInt(input.slice(0, 2), 10);
    const mm = parseInt(input.slice(2, 4), 10);
    if (!Number.isFinite(hh) || !Number.isFinite(mm)) return null;
    if (hh < 0 || hh > 23 || mm < 0 || mm > 59) return null;
    const now = new Date();
    const target = new Date(
      Date.UTC(
        now.getUTCFullYear(),
        now.getUTCMonth(),
        now.getUTCDate(),
        hh,
        mm,
        0,
        0,
      ),
    );
    let dur = target.getTime() - now.getTime();
    // Already-passed (including exact match) → schedule for tomorrow.
    if (dur <= 0) dur += 24 * 60 * 60_000;
    return dur;
  }
}
