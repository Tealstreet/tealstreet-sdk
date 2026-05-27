import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { OrderSide, OrderType, OrderTimeInForce, type PlaceOrderOpts } from '@tealstreet/safe-cex/src/types';
import { adjust } from '@tealstreet/safe-cex/src/utils/safe-math';

/**
 * `swarm <side> <size> into <count> [reduce] [irregular]` — fire N
 * market orders that together sum to <size>. Equal-sized slices by
 * default; with `irregular`, sizes are randomly distributed around
 * the mean (each slice in [0.5x .. 1.5x of mean], normalised).
 *
 * All orders fire concurrently via `Promise.all` — the swarm is a
 * one-shot fragmentation, not a long-running task. If you want pacing
 * use `twap` instead.
 *
 * Sizing rules follow Phase 0 decision 1: only `$N` / `N$` / `N%` /
 * `N%bal` / `N%pos` accepted. Bare numbers throw.
 */
export class SwarmCommand extends TradingBaseCommand {
  readonly name = 'swarm';
  readonly description = 'Fire N market orders that sum to <size>';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples = [
    'swarm buy $1000 into 5',
    'swarm sell $500 into 10 reduce',
    'swarm buy $1000 into 5 irregular',
    'swarm sell $1000 into 10 irregular reduce ETHUSDT',
  ];
  readonly requiresConnection = true;

  /** Exposed for testing — overridable so tests can pin distribution. */
  protected randomMultiplier(): number {
    return 0.5 + Math.random();
  }

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) return this.error('Not connected to exchange');
    if (!exchange.placeOrder) return this.error('Exchange does not support placing orders');

    const args = this.getRawArgs(ctx);

    // Side
    const sideStr = args[1]?.toLowerCase();
    if (sideStr !== 'buy' && sideStr !== 'sell') {
      return this.error("swarm: first argument must be 'buy' or 'sell'");
    }
    const side: OrderSide = sideStr === 'buy' ? OrderSide.Buy : OrderSide.Sell;

    // Optional leading symbol: `swarm buy SYMBOL $100 into 5`
    let i = 2;
    let symbol = ctx.symbol;
    const leading = this.validateSymbols(exchange, args[i]?.split(',') || []);
    if (leading.length > 0) {
      symbol = leading[0];
      i++;
    }

    // Size
    const sizeArg = args[i];
    if (!sizeArg) {
      return this.error('swarm: missing size');
    }
    i++;

    // `into <count>` is required
    if (args[i] !== 'into') {
      return this.error("swarm: expected 'into <count>' after size");
    }
    const count = parseInt(args[i + 1], 10);
    if (!Number.isFinite(count) || count <= 0) {
      return this.error("swarm: 'into' must be followed by a positive integer count");
    }
    i += 2;

    // Walk remaining args for reduce / irregular / trailing symbol
    let reduceOnly = false;
    let irregular = false;
    for (; i < args.length; i++) {
      const a = args[i];
      if (a === 'reduce') {
        reduceOnly = true;
        continue;
      }
      if (a === 'irregular') {
        irregular = true;
        continue;
      }
      const trailing = this.validateSymbols(exchange, [a]);
      if (trailing.length > 0) {
        symbol = trailing[0];
        continue;
      }
      return this.error(`swarm: unexpected token "${a}"`);
    }

    // Health gate — block opening exposure when the private stream is
    // degraded; reduce-only swarms reduce risk and stay allowed. Gated
    // after arg parsing so malformed swarms surface their parse error
    // rather than the degraded message (TSBUG-106).
    if (!reduceOnly) {
      const blocked = this.healthGate(ctx);
      if (blocked) return this.error(blocked);
    }

    // Resolve total contracts. For entries (non-reduce) % is balance; for
    // reduce-only % is position — same defaults as buy/sell vs close.
    let totalContracts: number;
    try {
      totalContracts = this.parseSizeAndConvertIfUsd(
        exchange,
        symbol,
        sizeArg,
        true,
        undefined,
        reduceOnly ? 'position' : 'balance',
      );
    } catch (e) {
      return this.error((e as Error).message);
    }

    if (totalContracts <= 0) {
      return this.error('swarm: total size resolved to zero contracts');
    }

    const market = exchange.store?.markets?.find((m) => m.symbol === symbol);
    if (!market) {
      return this.error(`Market not found for ${symbol}`);
    }

    const slices = this.distribute(totalContracts, count, irregular, market.precision.amount);
    if (slices.some((s) => s <= 0)) {
      return this.error(`swarm: total size too small to split into ${count} (precision ${market.precision.amount})`);
    }

    const orders: PlaceOrderOpts[] = [];
    for (const amount of slices) {
      if (amount <= 0) continue;
      orders.push({
        symbol,
        side,
        type: OrderType.Market,
        amount,
        reduceOnly,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });
    }

    // Fatfinger — per-slice. Skip for reduce-only swarms (closing).
    // Budget is aggregate (whole swarm's notional), checked before but
    // charged AFTER placement, proportional to the slices that landed.
    let budgetNotional = 0;
    if (!reduceOnly) {
      try {
        for (const o of orders) {
          this.enforceFatfinger(ctx, symbol, o.amount);
        }
        budgetNotional = this.checkBudget(ctx, exchange, symbol, totalContracts);
      } catch (e) {
        return this.error((e as Error).message);
      }
    }

    // Fire all simultaneously. Errors on individual orders are gathered
    // so a single bad slice doesn't stop the rest.
    const results = await Promise.allSettled(orders.map((o) => exchange.placeOrder!(o)));
    const ok = results.filter((r) => r.status === 'fulfilled').length;
    const fail = results.length - ok;
    // Charge the budget for exactly the slices that placed — sum their
    // own amounts (results is index-aligned with orders), so irregular
    // swarms with unequal slices are charged accurately, not prorated by
    // count. Failed slices consume nothing.
    if (budgetNotional > 0 && ok > 0) {
      let placedAmount = 0;
      results.forEach((r, i) => {
        if (r.status === 'fulfilled') placedAmount += Math.abs(orders[i].amount);
      });
      const charged = this.sizeToNotional(exchange, symbol, placedAmount);
      if (charged > 0) ctx.preprocessStore?.addBudgetSpent(charged);
    }
    // Accumulate IDs from successful slices for the capture path. A
    // failed slice contributes nothing — partial captures are honest
    // about what actually landed. Each placeOrder returns string[]
    // (multi-ID per call rare for swarm, but defensive).
    const orderIds: string[] = [];
    for (const r of results) {
      if (r.status === 'fulfilled' && Array.isArray(r.value)) {
        orderIds.push(...r.value);
      }
    }

    if (fail === 0) {
      this.printSuccess(ctx, `Swarm placed: ${ok} ${symbol} ${sideStr} order(s)`);
    } else if (ok > 0) {
      this.printWarning(ctx, `Swarm partial: ${ok} placed, ${fail} failed`);
    } else {
      return this.error('Swarm: all orders failed to place');
    }
    return this.successWithData({ orderIds });
  }

  /** Split `total` into `count` slices, snapped to `precision`. */
  protected distribute(total: number, count: number, irregular: boolean, precision: number): number[] {
    if (!irregular) {
      const equal = total / count;
      const sliced = Array(count)
        .fill(0)
        .map(() => adjust(equal, precision));
      // Last slice absorbs the rounding remainder so the sum lands on total.
      const sum = sliced.reduce((a, b) => a + b, 0);
      const delta = adjust(total - sum, precision);
      sliced[sliced.length - 1] = adjust(sliced[sliced.length - 1] + delta, precision);
      return sliced;
    }
    // Irregular: random weights in [0.5, 1.5], then renormalise.
    const weights = Array.from({ length: count }, () => this.randomMultiplier());
    const wSum = weights.reduce((a, b) => a + b, 0);
    const sliced = weights.map((w) => adjust((w / wSum) * total, precision));
    // Re-balance the last slice to absorb the rounding sum drift.
    const sum = sliced.reduce((a, b) => a + b, 0);
    const delta = adjust(total - sum, precision);
    sliced[sliced.length - 1] = adjust(sliced[sliced.length - 1] + delta, precision);
    return sliced;
  }
}
