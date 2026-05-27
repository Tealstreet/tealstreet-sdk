import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `whitelist`            — list whitelisted symbols
 * `whitelist <symbol>`   — add a symbol (resolves through the focused
 *                          exchange's market list when present)
 *
 * `unwhitelist <symbol>` — remove a symbol
 * `unwhitelist all`      — clear the whitelist
 *
 * Whitelisted symbols are skipped by global commands that walk every
 * position — `close longs`, `close shorts`, `close allpos`, `close all`.
 * Persisted via `PreprocessStore` (file-backed in CLI, atom-backed in web).
 */
export class WhitelistCommand extends BaseCommand {
  readonly name = 'whitelist';
  readonly description = 'Whitelist a symbol from global "all positions" commands';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'whitelist',
    'whitelist BTCUSDT',
    'whitelist ethusdt',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('whitelist: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);

    if (!target) {
      const list = ctx.preprocessStore.getWhitelist();
      if (list.length === 0) {
        this.printInfo(ctx, '(whitelist is empty)');
        return this.success();
      }
      this.print(ctx, list.map((s) => `  ${s}`));
      return this.success();
    }

    const symbol = target.toUpperCase();
    if (ctx.preprocessStore.isWhitelisted(symbol)) {
      this.printInfo(ctx, `${symbol} is already whitelisted`);
      return this.success();
    }
    ctx.preprocessStore.addToWhitelist(symbol);
    this.printSuccess(ctx, `Whitelisted ${symbol}`);
    return this.success();
  }
}

export class UnwhitelistCommand extends BaseCommand {
  readonly name = 'unwhitelist';
  readonly description = 'Remove a symbol from the whitelist (or `unwhitelist all`)';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'unwhitelist BTCUSDT',
    'unwhitelist all',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('unwhitelist: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    if (!target) {
      this.printError(ctx, 'Usage: unwhitelist <symbol>  or  unwhitelist all');
      return this.error('Missing target');
    }

    if (target.toLowerCase() === 'all') {
      const before = ctx.preprocessStore.getWhitelist().length;
      ctx.preprocessStore.clearWhitelist();
      this.printSuccess(ctx, `Cleared whitelist (${before} symbol${before === 1 ? '' : 's'})`);
      return this.success();
    }

    const symbol = target.toUpperCase();
    const removed = ctx.preprocessStore.removeFromWhitelist(symbol);
    if (!removed) {
      this.printWarning(ctx, `${symbol} was not whitelisted`);
      return this.success();
    }
    this.printSuccess(ctx, `Removed ${symbol} from whitelist`);
    return this.success();
  }
}
