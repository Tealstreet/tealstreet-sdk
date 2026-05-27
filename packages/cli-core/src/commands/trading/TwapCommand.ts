import { OrderSide } from '@tealstreet/safe-cex/src/types';
import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { twapManager } from '@tealstreet/safe-cex/src/webworker/twap/TwapManager';
import { TwapStatus } from '@tealstreet/safe-cex/src/webworker/twap/TwapStatus';
import { GREEN, RED, YELLOW, CYAN, RESET, DIM, BOLD } from './colors';
import { formatPrice } from '@tealstreet/utils/src/formatting/formatPrice';

/**
 * Parse duration string to milliseconds
 * Supports: 30s, 5m, 1h, 2h30m, etc.
 */
function parseDuration(duration: string): number {
  const regex = /(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?/i;
  const match = duration.match(regex);
  if (!match || !match[0]) {
    throw new Error(`Invalid duration format: ${duration}. Use formats like: 30s, 5m, 1h, 1h30m`);
  }

  const hours = parseInt(match[1] || '0', 10);
  const minutes = parseInt(match[2] || '0', 10);
  const seconds = parseInt(match[3] || '0', 10);

  if (hours === 0 && minutes === 0 && seconds === 0) {
    throw new Error(`Invalid duration format: ${duration}. Use formats like: 30s, 5m, 1h, 1h30m`);
  }

  return hours * 3600000 + minutes * 60000 + seconds * 1000;
}

/**
 * Format duration in ms to human readable string
 */
function formatDuration(ms: number): string {
  const hours = Math.floor(ms / 3600000);
  const minutes = Math.floor((ms % 3600000) / 60000);
  const seconds = Math.floor((ms % 60000) / 1000);

  const parts: string[] = [];
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);
  if (seconds > 0 || parts.length === 0) parts.push(`${seconds}s`);

  return parts.join('');
}

/**
 * TWAP Command - Execute Time-Weighted Average Price orders
 *
 * Syntax:
 *   twap buy $1000 BTCUSDT duration=1h slices=10
 *   twap sell $500 duration=30m
 *   twap list
 *   twap stop <id>
 *   twap stop all
 */
export class TwapCommand extends TradingBaseCommand {
  readonly name = 'twap';
  readonly description = 'Execute TWAP (Time-Weighted Average Price) order';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples = [
    // Positional form (spec): `into <n> over <minutes>`.
    'twap buy $1000 into 10 over 60',
    'twap buy $1000 BTCUSDT into 10 over 60',
    'twap sell $500 into 5 over 30 reduce',
    'twap sell $500 into 5 over 30 chaser',
    // Phase 4: chaser-per-slice with timeout/fail policy.
    'twap chase buy $1000 into 10 over 60',
    'twap chase buy $1000 BTCUSDT into 10 over 60',
    'twap chase buy $1000 into 10 over 60 timeout 30% next',
    'twap chase sell $500 into 5 over 30 reduce fail next',
    // Legacy key=value form (kept for v2 backwards compat).
    'twap buy $1000 duration=1h slices=10',
    'twap list',
    'twap stop <id>',
    'twap stop all',
  ];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const args = this.getRawArgs(ctx);
    const subcommand = args[1]?.toLowerCase();

    // Handle list/stop subcommands — these don't write to the
    // exchange, so they bypass the simulation gate. (Stop is local
    // task management, not order placement.)
    if (subcommand === 'list') {
      return this.handleList(ctx);
    }
    if (subcommand === 'stop') {
      return this.handleStop(ctx, args[2]);
    }

    // Everything past this point spawns a TWAP task that will fire
    // orders, so the simulation gate applies.
    if (this.simulationGate(ctx)) return this.success();

    // `twap chase <buy|sell> ...` — chaser-per-slice TWAP with
    // optional timeout/fail policy. Defaults: timeout 50% market,
    // fail market.
    if (subcommand === 'chase') {
      return this.handleChaseBuySell(ctx);
    }

    // Handle buy/sell
    if (subcommand !== 'buy' && subcommand !== 'sell') {
      return this.error(
        `Unknown subcommand "${args[1]}". Use: twap buy, twap sell, twap chase, twap list, or twap stop`,
      );
    }

    return this.handleBuySell(ctx, subcommand as OrderSide);
  }

  /**
   * Parse `twap chase <side> <size> [inst] into <n> over <m>
   *   [timeout <pct%> <market|next>] [fail <market|next>] [reduce]`.
   *
   * Defaults from spec: timeout 50% market, fail market.
   */
  private handleChaseBuySell(ctx: CommandContext): CommandResult {
    const args = this.getRawArgs(ctx);
    const sideStr = args[2]?.toLowerCase();
    if (sideStr !== 'buy' && sideStr !== 'sell') {
      return this.error("twap chase: missing side (use 'buy' or 'sell')");
    }
    const side: OrderSide = sideStr === 'buy' ? OrderSide.Buy : OrderSide.Sell;

    // Parse timeout / fail keywords first, then drop their tokens so
    // the existing `handleBuySell` machinery can see a clean argv.
    let timeoutPct: number | undefined;
    let timeoutAction: 'market' | 'next' | undefined;
    let failAction: 'market' | 'next' | undefined;

    const timeoutIdx = args.indexOf('timeout');
    if (timeoutIdx !== -1) {
      const pctArg = args[timeoutIdx + 1];
      const actionArg = args[timeoutIdx + 2];
      if (!pctArg || !/^\d+(?:\.\d+)?%$/.test(pctArg)) {
        return this.error(`twap chase timeout: expected '<pct%>' got '${pctArg}'`);
      }
      timeoutPct = parseFloat(pctArg);
      if (timeoutPct <= 0 || timeoutPct > 100) {
        return this.error(`twap chase timeout: ${timeoutPct}% must be in (0, 100]`);
      }
      if (actionArg !== 'market' && actionArg !== 'next') {
        return this.error(`twap chase timeout: action must be 'market' or 'next' (got '${actionArg}')`);
      }
      timeoutAction = actionArg;
    }
    const failIdx = args.indexOf('fail');
    if (failIdx !== -1) {
      const actionArg = args[failIdx + 1];
      if (actionArg !== 'market' && actionArg !== 'next') {
        return this.error(`twap chase fail: action must be 'market' or 'next' (got '${actionArg}')`);
      }
      failAction = actionArg;
    }

    // Apply spec defaults if not specified.
    const finalTimeoutPct = timeoutPct ?? 50;
    const finalTimeoutAction = timeoutAction ?? 'market';
    const finalFailAction = failAction ?? 'market';

    // Build a clean inner argv that `handleBuySell` can parse, with the
    // timeout/fail tokens stripped. We keep `into`/`over`/`reduce` and
    // any size/instrument tokens.
    const cleanedArgs = stripKeywordPairs(args.slice(2), [
      { key: 'timeout', skipNext: 2 },
      { key: 'fail', skipNext: 1 },
    ]);
    // Re-frame so cleanedArgs[0] is 'twap', [1] is 'buy'/'sell', etc.
    // — the same shape handleBuySell expects.
    const reframed = ['twap', ...cleanedArgs];
    const innerCtx: CommandContext = {
      ...ctx,
      args: { ...ctx.args, _: reframed },
    };

    const innerResult = this.handleBuySell(innerCtx, side);

    if (!innerResult.success) return innerResult;

    // Patch the produced taskAction with the chaser policy fields. The
    // 'chaser' flag is forced on because twap chase always uses
    // chasers per slice.
    if (innerResult.taskAction?.type === 'twap' && innerResult.taskAction.params) {
      const params = innerResult.taskAction.params as { params?: Record<string, unknown> };
      if (params.params) {
        params.params.useChaserOrders = true;
        params.params.timeoutPct = finalTimeoutPct;
        params.params.timeoutAction = finalTimeoutAction;
        params.params.failAction = finalFailAction;
      }
    }
    return innerResult;
  }

  /**
   * Handle twap list command
   */
  private handleList(ctx: CommandContext): CommandResult {
    const runningTwaps = Array.from(twapManager.runningTwaps.values());

    if (runningTwaps.length === 0) {
      this.printInfo(ctx, 'No TWAP orders running');
      return this.success();
    }

    const lines: string[] = [];
    lines.push(`${BOLD}Active TWAP Orders${RESET}`);
    lines.push('');

    for (const twap of runningTwaps) {
      const { options, state, id } = twap;
      const shortId = id.slice(0, 8);
      const sideColor = options.side === OrderSide.Buy ? GREEN : RED;
      const sideStr = options.side === OrderSide.Buy ? 'BUY' : 'SELL';

      // Status color
      let statusColor = DIM;
      let statusStr: string = state.status;
      if (state.status === TwapStatus.Running) {
        statusColor = GREEN;
        statusStr = 'running';
      } else if (state.status === TwapStatus.Paused) {
        statusColor = YELLOW;
        statusStr = 'paused';
      } else if (state.status === TwapStatus.Completed) {
        statusColor = CYAN;
        statusStr = 'completed';
      }

      // Progress
      const progress =
        options.ordersCount > 0 ? ((state.ordersExecuted / options.ordersCount) * 100).toFixed(1) : '0.0';

      // Time remaining (use nextOrderAt to estimate remaining time)
      const sliceInterval = options.duration / options.ordersCount;
      const remainingSlices = options.ordersCount - state.ordersExecuted;
      const remaining = Math.max(0, remainingSlices * sliceInterval);
      const remainingStr = remaining > 0 ? formatDuration(remaining) : 'done';

      lines.push(`  ${DIM}[${shortId}]${RESET} ${sideColor}${sideStr}${RESET} ${options.activeMarket.symbol}`);
      lines.push(`    Progress: ${state.ordersExecuted}/${options.ordersCount} slices (${progress}%)`);
      lines.push(`    Remaining: ${remainingStr}  Status: ${statusColor}${statusStr}${RESET}`);
      lines.push('');
    }

    this.print(ctx, lines);
    return this.success();
  }

  /**
   * Handle twap stop command
   */
  private handleStop(ctx: CommandContext, target?: string): CommandResult {
    if (!target) {
      return this.error('Please specify a TWAP ID to stop or "all" to stop all');
    }

    if (target.toLowerCase() === 'all') {
      const runningTwaps = Array.from(twapManager.runningTwaps.values()).filter(
        (t) => t.state.status !== TwapStatus.Completed,
      );

      if (runningTwaps.length === 0) {
        this.printInfo(ctx, 'No TWAP orders to stop');
        return this.success();
      }

      // Return taskAction to stop all
      return {
        success: true,
        message: `Stopping ${runningTwaps.length} TWAP order(s)`,
        taskAction: {
          action: 'stop',
          type: 'twap',
          taskIds: runningTwaps.map((t) => t.id),
        },
      };
    }

    // Find TWAP by ID prefix
    const twap = Array.from(twapManager.runningTwaps.values()).find((t) => t.id.startsWith(target.toLowerCase()));

    if (!twap) {
      return this.error(`No TWAP found with ID starting with "${target}"`);
    }

    if (twap.state.status === TwapStatus.Completed) {
      return this.error(`TWAP [${twap.id.slice(0, 8)}] is already completed`);
    }

    return {
      success: true,
      message: `Stopping TWAP [${twap.id.slice(0, 8)}]`,
      taskAction: {
        action: 'stop',
        type: 'twap',
        taskId: twap.id,
      },
    };
  }

  /**
   * Handle twap buy/sell command
   */
  private handleBuySell(ctx: CommandContext, side: OrderSide): CommandResult {
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }

    const args = this.getRawArgs(ctx);

    // Parse options from args
    let duration: number | undefined;
    let slices: number | undefined;
    let useChaserOrders = false;
    // `reduce` makes every slice a reduce-only order (TwapInstance reads
    // options.reduceOnly per slice) and also lets the fatfinger gate skip
    // the check on close-style TWAPs. Must be threaded into the emitted
    // task params below or the runtime never sees it.
    let reduceOnly = false;

    // Positional form (spec, Phase 3): `into <n> over <minutes>`.
    const intoIdx = args.indexOf('into');
    if (intoIdx !== -1) {
      const v = parseInt(args[intoIdx + 1], 10);
      if (!Number.isFinite(v) || v <= 0) {
        return this.error(`twap: 'into' must be followed by a positive integer slice count`);
      }
      slices = v;
    }
    const overIdx = args.indexOf('over');
    if (overIdx !== -1) {
      const v = parseFloat(args[overIdx + 1]);
      if (!Number.isFinite(v) || v <= 0) {
        return this.error(`twap: 'over' must be followed by a positive number of minutes`);
      }
      duration = v * 60_000;
    }

    // Legacy key=value options (still accepted for v2 backwards compat).
    for (const arg of args) {
      if (arg.startsWith('duration=')) {
        duration = parseDuration(arg.slice(9));
      } else if (arg.startsWith('slices=')) {
        slices = parseInt(arg.slice(7), 10);
      } else if (arg === 'chaser') {
        useChaserOrders = true;
      } else if (arg === 'reduce') {
        reduceOnly = true;
      }
    }

    // Validate required options
    if (!duration) {
      return this.error('Duration is required. Usage: twap buy $100 duration=1h slices=10');
    }
    if (!slices || slices < 1) {
      slices = 10; // Default to 10 slices
    }

    // Find size and symbol from args
    let sizeArg: string | undefined;
    let symbol: string = ctx.symbol;

    // Check for symbol at start: "twap buy SYMBOL $100 ..."
    const symbolsAtStart = this.validateSymbols(exchange, args[2]?.split(',') || []);
    if (symbolsAtStart.length > 0) {
      symbol = symbolsAtStart[0];
      sizeArg = args[3];
    } else if (args[2] && this.looksLikeSize(args[2])) {
      // "twap buy $100 ..."
      sizeArg = args[2];
      // Check for trailing symbol
      for (let i = args.length - 1; i >= 3; i--) {
        if (
          args[i].startsWith('duration=') ||
          args[i].startsWith('slices=') ||
          args[i] === 'chaser' ||
          args[i] === 'reduce'
        )
          continue;
        const validated = this.validateSymbols(exchange, [args[i]]);
        if (validated.length > 0) {
          symbol = validated[0];
          break;
        }
      }
    } else {
      return this.error('Size argument missing. Usage: twap buy $100 duration=1h slices=10');
    }

    if (!sizeArg || !this.looksLikeSize(sizeArg)) {
      return this.error('Size must be in USD format (e.g., $100). Usage: twap buy $100 duration=1h');
    }

    // Get market for the symbol
    const market = exchange.store?.markets?.find((m) => m.symbol === symbol);
    if (!market) {
      return this.error(`Market not found for symbol ${symbol}`);
    }

    // Health gate — don't start an exposure-increasing TWAP into a
    // degraded private stream; reduce-only TWAPs stay allowed. Gated
    // after parse so malformed `twap` surfaces its parse error (TSBUG-106).
    if (!reduceOnly) {
      const blocked = this.healthGate(ctx);
      if (blocked) return this.error(blocked);
    }

    // Convert USD to contract size
    const totalSize = this.parseSizeAndConvertIfUsd(exchange, symbol, sizeArg);

    // Fatfinger — gate on per-slice size. Total / slices is the
    // amount each individual order will request. Skip on reduce-only.
    if (!reduceOnly && slices > 0) {
      try {
        this.enforceFatfinger(ctx, symbol, totalSize / slices);
      } catch (e) {
        return this.error((e as Error).message);
      }
    }

    // Build task params
    const durationMs = duration;
    const totalSizeNum = totalSize;

    this.printInfo(
      ctx,
      `Starting TWAP ${side} ${formatPrice(totalSizeNum, market.precision.amount)} ${symbol} over ${formatDuration(durationMs)} in ${slices} slices${useChaserOrders ? ' (using chaser orders)' : ''}`,
    );

    return {
      success: true,
      taskAction: {
        action: 'start',
        type: 'twap',
        params: {
          symbol,
          side,
          account: ctx.accountName,
          status: 'pending',
          createdAt: Date.now(),
          progress: {
            filled: 0,
            total: totalSizeNum,
            percentage: 0,
          },
          params: {
            totalSize: totalSizeNum,
            duration: durationMs,
            slices,
            ordersExecuted: 0,
            useChaserOrders,
            reduceOnly,
          },
        },
      },
    };
  }
}

/** Drop runs of `<key> X [Y...]` from `args`. `skipNext` says how many
 *  tokens to drop after the key. Used by `twap chase` to cleanly remove
 *  `timeout 30% market` / `fail next` from the argv before reusing
 *  `handleBuySell`. */
function stripKeywordPairs(args: string[], pairs: Array<{ key: string; skipNext: number }>): string[] {
  const out: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const hit = pairs.find((p) => p.key === args[i]);
    if (hit) {
      i += hit.skipNext;
      continue;
    }
    out.push(args[i]);
  }
  return out;
}
