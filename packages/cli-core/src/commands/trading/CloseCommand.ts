import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { nativeCloseSize, prepareCliOrder } from '../../order/prepareCliOrder';
import { OrderSide, OrderType, PlaceOrderOpts, PositionSide } from '@tealstreet/safe-cex/src/types';

/**
 * Close command - closes positions, optionally partial.
 *
 * Forms:
 *   close                                — fully close focused symbol
 *   close <SYMBOL[,SYMBOL]>              — fully close named symbols
 *   close long | short [SYMBOL]          — fully close one hedge side
 *   close longs                          — close ALL long positions (global)
 *   close shorts                         — close ALL short positions (global)
 *   close allpos | close all             — close every open position
 *   close longs <size>                   — partial-close every long
 *   close shorts <size> / close allpos <size> — same for shorts / all
 *   close <size> [SYMBOL]                — partial close. Size may be
 *     `$100` / `100$` / `50%` / `50%bal` / `50%pos`. Bare numbers
 *     throw per Phase 0 decision 1.
 *
 * Partial closes are reduce-only market orders sized through
 * `parseSizeAndConvertIfUsd` with the position-scope default. If
 * there are two positions on a hedge symbol and no `long`/`short`
 * disambiguator, partial close errors rather than guessing.
 */
export class CloseCommand extends TradingBaseCommand {
  readonly name = 'close';
  readonly description = 'Close a position fully or partially';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['c'];
  readonly examples = [
    'close',
    'close short',
    'close short WIFUSDT',
    'close short WIFUSDT,ETHUSDT',
    'close long',
    'close long BTCUSDT',
    'close longs',
    'close shorts',
    'close allpos',
    'close all',
    'close longs 50%',
    'close BTCUSDT',
    'close 50%',
    'close $100',
    'close 100$ ETHUSDT',
  ];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }
    if (!exchange.placeOrder) {
      return this.error('Exchange does not support placing orders');
    }

    const args = this.getRawArgs(ctx);

    // Global "close every position" / side-filtered variants. These run
    // before the per-symbol path below so a `longs` / `shorts` / `allpos`
    // / `all` head doesn't fall into the symbol-resolution code that
    // assumes a single focused symbol.
    const head = (args[1] ?? '').toLowerCase();
    if (head === 'longs' || head === 'shorts' || head === 'allpos' || head === 'all') {
      return this.executeGlobalClose(ctx, exchange, args, head);
    }

    const onlyLongs = this.hasArgValue(args, 'long');
    const onlyShorts = this.hasArgValue(args, 'short');

    if (onlyLongs && onlyShorts) {
      return this.error('Cannot have both long and short closing requests');
    }

    // Detect a partial-close size token. It can appear as args[1] or
    // (after long/short) args[2]. Bare numbers are deliberately not
    // detected here — they fall through to the symbol-validate path
    // and end up in "no position" land if not a symbol.
    const sizeArg = this.findSizeToken(args);

    const possibleSymbols = [
      ...this.validateSymbols(
        exchange,
        sizeArg && args[1] === sizeArg ? [] : args[1]?.split(',') || [],
      ),
      ...this.validateSymbols(
        exchange,
        sizeArg && args[2] === sizeArg ? [] : args[2]?.split(',') || [],
      ),
      ...this.validateSymbols(
        exchange,
        sizeArg && args[3] === sizeArg ? [] : args[3]?.split(',') || [],
      ),
    ];

    if (possibleSymbols.length === 0) {
      possibleSymbols.push(ctx.symbol);
    }

    const positions = exchange.store?.positions ?? [];

    for (const symbol of possibleSymbols) {
      const symbolPositions = positions.filter(
        (position) =>
          position.symbol === symbol &&
          position.contracts !== 0 &&
          (!onlyShorts && !onlyLongs
            ? true
            : onlyShorts
              ? position.side === PositionSide.Short
              : onlyLongs
                ? position.side === PositionSide.Long
                : false),
      );

      if (sizeArg && symbolPositions.length > 1) {
        return this.error(
          `Multiple positions on ${symbol} — use 'close long <size>' or 'close short <size>' to disambiguate`,
        );
      }

      for (const position of symbolPositions) {
        let amount: number;
        if (sizeArg) {
          try {
            amount = this.parseSizeAndConvertIfUsd(
              exchange,
              symbol,
              sizeArg,
              true,
              position.side,
              'position',
            );
          } catch (e) {
            return this.error((e as Error).message);
          }
          // Cap at the position size — `close 200%` should not flip the
          // position. Both sides must be NATIVE for the comparison to mean
          // anything; `position.contracts` is base.
          const capped = Math.min(Math.abs(amount), nativeCloseSize(exchange, position));
          amount = capped;
        } else {
          amount = nativeCloseSize(exchange, position);
        }

        const order: PlaceOrderOpts = {
          symbol,
          side: position.side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.Market,
          amount,
          reduceOnly: true,
        };

        await exchange.placeOrder(prepareCliOrder(exchange, order));
      }
    }

    this.printSuccess(ctx, sizeArg ? 'Partial close placed' : 'Positions closed');
    return this.success();
  }

  /** Returns the first arg that looks like a USD size or percent. */
  private findSizeToken(args: string[]): string | null {
    for (let i = 1; i < args.length; i++) {
      const a = args[i];
      if (!a) continue;
      if (this.looksLikeSize(a)) return a;
      if (/^\d+(\.\d+)?(%|%bal|%pos)$/.test(a)) return a;
    }
    return null;
  }

  private async executeGlobalClose(
    ctx: CommandContext,
    exchange: NonNullable<CommandContext['exchange']>,
    args: string[],
    head: 'longs' | 'shorts' | 'allpos' | 'all' | string,
  ): Promise<CommandResult> {
    const placeOrder = exchange.placeOrder;
    // execute() already gated on placeOrder being defined, but the
    // type narrowing doesn't carry across the helper boundary.
    if (!placeOrder) {
      return this.error('Exchange does not support placing orders');
    }
    const sizeArg = this.findSizeToken(args);
    const allOpen = (exchange.store?.positions ?? []).filter((p) => p.contracts !== 0);

    let targets = allOpen;
    if (head === 'longs') {
      targets = allOpen.filter((p) => p.side === PositionSide.Long);
    } else if (head === 'shorts') {
      targets = allOpen.filter((p) => p.side === PositionSide.Short);
    }
    // 'allpos' / 'all' → every open position, no filter.

    // Drop whitelisted symbols before counting / firing. Whitelist is
    // a soft safety net for "I never want this auto-touched" — most
    // useful for long-term holds that share an exchange with active
    // trading positions.
    const store = ctx.preprocessStore;
    const skipped: string[] = [];
    if (store) {
      const before = targets;
      targets = before.filter((p) => {
        if (store.isWhitelisted(p.symbol)) {
          skipped.push(p.symbol);
          return false;
        }
        return true;
      });
    }

    if (targets.length === 0) {
      const skippedNote = skipped.length > 0 ? ` (skipped whitelisted: ${[...new Set(skipped)].sort().join(', ')})` : '';
      this.printInfo(
        ctx,
        `(no ${head === 'allpos' || head === 'all' ? 'open' : head} positions to close)${skippedNote}`,
      );
      return this.success();
    }
    if (skipped.length > 0) {
      this.printWarning(
        ctx,
        `Skipping whitelisted: ${[...new Set(skipped)].sort().join(', ')}`,
      );
    }

    let placed = 0;
    for (const position of targets) {
      // Per-position: a global close walks every open position, and one symbol
      // whose market has not loaded must not abort the rest of the sweep.
      //
      // SIZING is inside the try, not just the placement. `nativeCloseSize`
      // refuses a missing market, and it used to sit outside this block — so
      // one unloaded symbol would have thrown past the catch and killed the
      // sweep, which is the regression this comment was written about.
      try {
        const full = nativeCloseSize(exchange, position);
        const amount = sizeArg
          ? Math.min(
              Math.abs(
                this.parseSizeAndConvertIfUsd(exchange, position.symbol, sizeArg, true, position.side, 'position'),
              ),
              full,
            )
          : full;

        const order: PlaceOrderOpts = {
          symbol: position.symbol,
          side: position.side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.Market,
          amount,
          reduceOnly: true,
        };

        await placeOrder(prepareCliOrder(exchange, order));
        placed++;
      } catch (e) {
        this.printError(ctx, `${position.symbol}: ${(e as Error).message}`);
      }
    }

    const label =
      head === 'longs'
        ? 'long'
        : head === 'shorts'
          ? 'short'
          : 'open';
    this.printSuccess(
      ctx,
      sizeArg
        ? `Partial close placed on ${placed} ${label} position(s)`
        : `Closing ${placed} ${label} position(s)`,
    );
    return this.success();
  }
}
