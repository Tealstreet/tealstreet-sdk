import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { OrderSide, OrderStatus, OrderType, type Order } from '@tealstreet/safe-cex/src/types';
import { ID_SUFFIX_RE } from '../../preprocess/parserUtils';

const FILTER_KEYWORDS = [
  'buys', 'sells', 'stops', 'limits', 'buy', 'sell',
  'top', 'bottom', 'first', 'last',
  'ro', 'xro', 'from', 'to',
];

function isLimit(o: Order): boolean {
  return o.type === OrderType.Limit;
}

function isStop(o: Order): boolean {
  return o.type !== OrderType.Limit && o.type !== OrderType.Market;
}

/**
 * Cancel command - cancels orders.
 *
 * Filters compose:
 *   - side:     buys / sells
 *   - kind:     limits / stops (default: limits)
 *   - reduce:   ro (reduce-only) / xro (excluding reduce-only)
 *   - position: top N / bottom N (by price); first N / last N (by age)
 *   - range:    from <p1> to <p2> (price-range; limits only)
 *   - id:       a trailing ID match (3+ chars of [A-Za-z0-9_-])
 *
 * Symbol scope is taken from the trailing arg if it resolves; otherwise
 * the focused symbol. Filter keywords are skipped during that lookup.
 */
export class CancelCommand extends TradingBaseCommand {
  readonly name = 'cancel';
  readonly description = 'Cancel orders';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['x'];
  readonly examples = [
    'cancel',
    'cancel ETHUSDT',
    'cancel buys',
    'cancel buys ETHUSDT',
    'cancel sells',
    'cancel stops',
    'cancel limits',
    'cancel ro',
    'cancel xro',
    'cancel buys ro',
    'cancel sells xro',
    'cancel buy stops',
    'cancel top 3',
    'cancel bottom 5 ETHUSDT',
    'cancel first 2',
    'cancel last 1',
    'cancel from 49000 to 50000',
    'cancel 1234abcd',
  ];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }

    const args = this.getRawArgs(ctx);
    const targetSymbol = this.getTargetSymbol(ctx, FILTER_KEYWORDS);
    const orders = exchange.store?.orders ?? [];
    const head = (args[1] ?? '').toLowerCase();

    // Top/bottom/first/last <n> — selectors over open limit orders
    if (['top', 'bottom', 'first', 'last'].includes(head)) {
      if (!exchange.cancelOrders) {
        return this.error('Exchange does not support canceling specific orders');
      }
      const n = parseInt(args[2], 10);
      if (!Number.isFinite(n) || n <= 0) {
        return this.error(`Usage: cancel ${head} <n> [SYMBOL]`);
      }
      const limitOrders = orders.filter(
        (o) =>
          o.symbol === targetSymbol &&
          o.type === OrderType.Limit &&
          o.status === OrderStatus.Open,
      );
      if (limitOrders.length === 0) {
        this.printInfo(ctx, `No open limit orders on ${targetSymbol}`);
        return this.success();
      }
      let picked: typeof limitOrders;
      switch (head) {
        case 'top':
          picked = [...limitOrders].sort((a, b) => b.price - a.price).slice(0, n);
          break;
        case 'bottom':
          picked = [...limitOrders].sort((a, b) => a.price - b.price).slice(0, n);
          break;
        case 'first':
          // Oldest first by lastUpdate (proxy for createdAt; safe-cex
          // doesn't expose a separate creation timestamp).
          picked = [...limitOrders].sort((a, b) => a.lastUpdate - b.lastUpdate).slice(0, n);
          break;
        case 'last':
        default:
          picked = [...limitOrders].sort((a, b) => b.lastUpdate - a.lastUpdate).slice(0, n);
          break;
      }
      await exchange.cancelOrders(picked);
      this.printSuccess(ctx, `Cancelling ${picked.length} ${head} ${targetSymbol} order(s)`);
      return this.success();
    }

    // `cancel from <p1> to <p2> [SYMBOL]` — price-range over limits.
    if (head === 'from') {
      if (!exchange.cancelOrders) {
        return this.error('Exchange does not support canceling specific orders');
      }
      const p1 = parseFloat(args[2]);
      const toKw = (args[3] ?? '').toLowerCase();
      const p2 = parseFloat(args[4]);
      if (!Number.isFinite(p1) || toKw !== 'to' || !Number.isFinite(p2)) {
        return this.error('Usage: cancel from <p1> to <p2> [SYMBOL]');
      }
      const lo = Math.min(p1, p2);
      const hi = Math.max(p1, p2);
      const inRange = orders.filter(
        (o) =>
          o.symbol === targetSymbol &&
          o.type === OrderType.Limit &&
          o.status === OrderStatus.Open &&
          o.price >= lo &&
          o.price <= hi,
      );
      if (inRange.length === 0) {
        this.printInfo(ctx, `No ${targetSymbol} limit orders in [${lo}, ${hi}]`);
        return this.success();
      }
      await exchange.cancelOrders(inRange);
      this.printSuccess(
        ctx,
        `Cancelling ${inRange.length} ${targetSymbol} order(s) in [${lo}, ${hi}]`,
      );
      return this.success();
    }

    // `cancel ro` / `cancel xro` (optionally combined with side).
    if (head === 'ro' || head === 'xro') {
      if (!exchange.cancelOrders) {
        return this.error('Exchange does not support canceling specific orders');
      }
      const wantRO = head === 'ro';
      const filtered = orders.filter(
        (o) =>
          o.symbol === targetSymbol &&
          o.type === OrderType.Limit &&
          (wantRO ? o.reduceOnly : !o.reduceOnly),
      );
      await exchange.cancelOrders(filtered);
      this.printSuccess(
        ctx,
        `Cancelling ${filtered.length} ${targetSymbol} ${wantRO ? 'reduce-only' : 'non-reduce-only'} order(s)`,
      );
      return this.success();
    }

    // `cancel limits` — non-stop limits only (explicit version of the
    // default symbol-scope cancel; useful when you want to keep stops).
    if (head === 'limits') {
      if (!exchange.cancelOrders) {
        return this.error('Exchange does not support canceling specific orders');
      }
      const lims = orders.filter((o) => o.symbol === targetSymbol && isLimit(o));
      await exchange.cancelOrders(lims);
      this.printSuccess(ctx, `Cancelling ${lims.length} ${targetSymbol} limit order(s)`);
      return this.success();
    }

    // Basic cancel (no filter or just a symbol). Symbol resolution falls back
    // to a base-prefix match, so `buys` resolves to BUSDT, `sells` / `stops`
    // to SUSDT and an id suffix like `b3f9` to BUSDT on venues listing a `B`
    // or `S` market. Cancel-all therefore needs a keyword-free token that
    // names the market exactly: its symbol (`ETHUSDT`) or its base (`eth`).
    // Anything else falls through to the id match below, or errors.
    const token = (args[1] ?? '').trim().toUpperCase();
    const resolved = token ? this.transformSymbol(exchange, token) : '';
    const resolvedBase = exchange.store?.markets?.find((m) => m.symbol === resolved)?.base;
    const namesMarket =
      !FILTER_KEYWORDS.includes(head) &&
      this.validateSymbol(exchange, resolved) &&
      (resolved.toUpperCase() === token || resolvedBase?.toUpperCase() === token);
    if (!args[1] || namesMarket) {
      if (exchange.cancelSymbolOrders) {
        await exchange.cancelSymbolOrders(targetSymbol);
      } else if (exchange.cancelOrders) {
        const symbolOrders = orders.filter((o) => o.symbol === targetSymbol);
        await exchange.cancelOrders(symbolOrders);
      } else {
        return this.error('Exchange does not support canceling orders');
      }
      this.printSuccess(ctx, `Cancelling all ${targetSymbol} orders`);
      return this.success();
    }

    // Check if cancelOrders is available for filtered cancels
    if (!exchange.cancelOrders) {
      return this.error('Exchange does not support canceling specific orders');
    }

    switch (head) {
      case 'buys':
      case 'sells': {
        const wantSide = head === 'buys' ? OrderSide.Buy : OrderSide.Sell;
        const sub = (args[2] ?? '').toLowerCase();
        const candidates = orders.filter(
          (o) =>
            o.symbol === targetSymbol &&
            o.type === OrderType.Limit &&
            o.side === wantSide,
        );
        let picked: typeof candidates;
        let label: string;
        if (sub === 'ro') {
          picked = candidates.filter((o) => o.reduceOnly);
          label = `${head} reduce-only`;
        } else if (sub === 'xro') {
          picked = candidates.filter((o) => !o.reduceOnly);
          label = `${head} non-reduce-only`;
        } else {
          picked = candidates;
          label = head;
        }
        await exchange.cancelOrders(picked);
        this.printSuccess(ctx, `Cancelling ${picked.length} ${targetSymbol} ${label} order(s)`);
        return this.success();
      }
      case 'stops': {
        const stops = orders.filter((o) => o.symbol === targetSymbol && isStop(o));
        await exchange.cancelOrders(stops);
        this.printSuccess(ctx, `Cancelling all ${targetSymbol} stops`);
        return this.success();
      }
      case 'buy':
      case 'sell': {
        if ((args[2] ?? '').toLowerCase() === 'stops') {
          const wantSide = head === 'buy' ? OrderSide.Buy : OrderSide.Sell;
          const sideStops = orders.filter(
            (o) => o.symbol === targetSymbol && o.side === wantSide && isStop(o),
          );
          await exchange.cancelOrders(sideStops);
          this.printSuccess(ctx, `Cancelling all ${targetSymbol} ${head} stops`);
          return this.success();
        }
        this.printError(ctx, 'Not implemented');
        return this.error('Not implemented');
      }
      default: {
        // Trailing-ID match — `cancel <suffix>` where suffix is 3+
        // [A-Za-z0-9_-] chars and isn't a recognised keyword. We fall
        // through to here, so the symbol-as-args[1] case above has
        // already been ruled out.
        //
        // Also handles comma-separated lists — `cancel <id1>,<id2>,…`
        // — which is what `cancel @<var>` becomes after substitution
        // when the var was populated via `@<var> = buy …` capture
        // (commit 3). Each piece must independently match
        // ID_SUFFIX_RE; otherwise we fall through to "Not implemented".
        const pieces = args[1].split(',').map((p) => p.trim()).filter((p) => p);
        if (pieces.length > 0 && pieces.every((p) => ID_SUFFIX_RE.test(p))) {
          const matches = orders.filter(
            (o) =>
              o.status === OrderStatus.Open &&
              pieces.some((p) => o.id === p || o.id.endsWith(p)),
          );
          if (matches.length === 0) {
            const label = pieces.length === 1 ? `id "${pieces[0]}"` : `ids ${pieces.map((p) => `"${p}"`).join(', ')}`;
            this.printError(ctx, `No open order matching ${label}`);
            return this.error('Order not found');
          }
          await exchange.cancelOrders(matches);
          this.printSuccess(
            ctx,
            pieces.length === 1
              ? matches.length === 1
                ? `Cancelling order ${matches[0].id}`
                : `Cancelling ${matches.length} orders matching "${pieces[0]}"`
              : `Cancelling ${matches.length} order(s) matching ${pieces.length} id(s)`,
          );
          return this.success();
        }
        this.printError(ctx, 'Not implemented');
        return this.error('Not implemented');
      }
    }
  }
}
