import { evaluateLiveVarSource } from '../../preprocess/reactiveVars';
import type { PreprocessContext, PreprocessStore } from '../../preprocess/types';
import type { ExchangeLike } from '../../types';
import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `print @<name>`  — display one variable's value.
 * `print all`      — display every variable (static AND reactive).
 * `print list`     — synonym for `print all`.
 *
 * Aliases:
 *   `vars`          — `print all`
 *   `vars list`     — `print all`
 *   `vars @<name>`  — `print @<name>`
 *
 * Static vars render as `@x = <value>`. Reactive vars render as
 * `@x (live) = <source>  →  <current-value>` so the user sees both
 * the live expression they registered and what it resolves to RIGHT
 * NOW. Evaluation errors (no position for `$entry`, no ticker for
 * `$ask`, etc.) surface as `→ (<error message>)` rather than blocking
 * the listing — `print` is informational.
 */
export class PrintCommand extends BaseCommand {
  readonly name = 'print';
  readonly description = 'Show a user variable, or list all (`print all` / `vars`)';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = ['vars'];
  readonly examples = [
    'print @size',
    'print all',
    'print list',
    'vars',
    'vars list',
    'vars @size',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('print: preprocess store unavailable');
    }
    const cmdName = (this.getArg(ctx, 0) ?? '').toLowerCase();
    const target = this.getArg(ctx, 1);

    // `vars` with no target → list all. `print` with no target stays
    // an error to preserve the existing usage hint.
    const wantListAll =
      target === 'all' ||
      target === 'list' ||
      (cmdName === 'vars' && !target);

    if (wantListAll) {
      const staticVars = ctx.preprocessStore.getVars();
      const liveVars = ctx.preprocessStore.getLiveVars();
      const trackedVars = ctx.preprocessStore.getTrackedVars();
      const totalCount =
        Object.keys(staticVars).length +
        Object.keys(liveVars).length +
        Object.keys(trackedVars).length;
      if (totalCount === 0) {
        this.printInfo(ctx, '(no variables set)');
        return this.success();
      }
      // Merge into a single sorted list across all three kinds.
      const lines: { name: string; line: string }[] = [];
      for (const [k, v] of Object.entries(staticVars)) {
        lines.push({ name: k, line: `@${k} = ${v}` });
      }
      for (const [k, source] of Object.entries(liveVars)) {
        lines.push({
          name: k,
          line: formatLiveVarLine(k, source, ctx),
        });
      }
      for (const [k, entry] of Object.entries(trackedVars)) {
        lines.push({
          name: k,
          line: formatTrackedVarLine(k, entry.kind, entry.source, entry.value),
        });
      }
      this.print(
        ctx,
        lines
          .sort((a, b) => a.name.localeCompare(b.name))
          .map(({ line }) => line),
      );
      return this.success();
    }

    if (!target) {
      this.printError(ctx, 'Usage: print @<name>  or  print all');
      return this.error('Missing target');
    }

    if (!target.startsWith('@') || target.length < 2) {
      this.printError(ctx, 'Usage: print @<name>  or  print all');
      return this.error('Invalid variable name');
    }

    const name = target.slice(1);
    // Tracker → live → static, matching substitute precedence.
    const trackedEntry = ctx.preprocessStore.getTrackedVar(name);
    if (trackedEntry) {
      this.print(ctx, [
        formatTrackedVarLine(name, trackedEntry.kind, trackedEntry.source, trackedEntry.value),
      ]);
      return this.success();
    }
    const liveSource = ctx.preprocessStore.getLiveVar(name);
    if (liveSource !== undefined) {
      this.print(ctx, [formatLiveVarLine(name, liveSource, ctx)]);
      return this.success();
    }
    const value = ctx.preprocessStore.getVar(name);
    if (value === undefined) {
      this.printWarning(ctx, `@${name} is not set`);
      return this.success();
    }
    this.print(ctx, [`@${name} = ${value}`]);
    return this.success();
  }
}

/**
 * Format a tracker line. `print` is informational, so we show the
 * STORED value (the high/low-water mark seen so far) rather than
 * re-evaluating the source — re-evaluation would mutate the
 * extreme as a side-effect of a read-only command, which is
 * surprising. The user can `echo @<name>` to force a re-eval +
 * update.
 *
 * Unseeded trackers (no resolution yet) render as `(unseeded)`.
 */
function formatTrackedVarLine(
  name: string,
  kind: 'min' | 'max',
  source: string,
  value: number | undefined,
): string {
  const valueStr = value === undefined ? '(unseeded)' : String(value);
  return `@${name} (track ${kind}) = ${source}  →  ${valueStr}`;
}

/**
 * Format one live-var line — source plus current evaluation. Failures
 * during evaluation are inlined as `→ (<msg>)` so the listing always
 * completes. We strip the outer `live var @<name>` wrapping from the
 * inner error message because the line already starts with `@<name>`
 * — no need to repeat the prefix.
 */
function formatLiveVarLine(
  name: string,
  source: string,
  ctx: CommandContext,
): string {
  const pctx: PreprocessContext = {
    exchange: ctx.exchange as ExchangeLike | null,
    symbol: ctx.symbol,
    store: ctx.preprocessStore as PreprocessStore,
    accountName: ctx.accountName,
  };
  let trailer: string;
  try {
    const value = evaluateLiveVarSource(name, source, pctx);
    trailer = String(value);
  } catch (err) {
    const raw = err instanceof Error ? err.message : String(err);
    // Strip the redundant `live var @<name> source '<src>':` prefix —
    // the listing line already shows both.
    const msg = raw.replace(
      new RegExp(`^live var @${name} source '[^']*':\\s*`),
      '',
    );
    trailer = `(${msg})`;
  }
  return `@${name} (live) = ${source}  →  ${trailer}`;
}
