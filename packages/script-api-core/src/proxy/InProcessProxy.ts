/**
 * v1 ApiProxy implementation — forwards directly to a host CoreAdapter.
 * See SPEC §11 Phase 1 + Phase 2.
 *
 * Dispatch is a one-case-per-method switch so the TS compiler verifies every
 * MethodName has a real handler. Adding a method to `ReadMethods` /
 * `WriteMethods` in ApiProxy.ts will surface a compile error here until
 * the case is wired.
 *
 * The proxy is **dumb**: capability checks fire in `buildApi` BEFORE the
 * proxy is touched, and account resolution runs in the same place. The
 * proxy only translates the `(method, args)` envelope into the matching
 * adapter call. Phase 8 swaps this for a postMessage transport without
 * changing the envelope shape.
 */
import type { CoreAdapter } from '../adapter/AdapterContext';

import type {
  ApiProxy,
  ReadMethodArgs,
  ReadMethodName,
  ReadMethodResult,
  WriteMethodArgs,
  WriteMethodName,
  WriteMethodResult,
} from './ApiProxy';

export class InProcessProxy implements ApiProxy {
  constructor(private readonly adapter: CoreAdapter) {}

  async call<M extends ReadMethodName>(method: M, args: ReadMethodArgs<M>): Promise<ReadMethodResult<M>> {
    switch (method) {
      case 'getActiveAccount':
        return (await this.adapter.getActiveAccount()) as ReadMethodResult<M>;
      case 'getActiveAccounts':
        return (await this.adapter.getActiveAccounts()) as ReadMethodResult<M>;
      case 'getActiveSymbol':
        return (await this.adapter.getActiveSymbol()) as ReadMethodResult<M>;
      case 'getActiveExchange':
        return (await this.adapter.getActiveExchange()) as ReadMethodResult<M>;
      case 'getAccounts':
        return (await this.adapter.getAccounts()) as ReadMethodResult<M>;

      case 'getMarkets': {
        const [opts] = args as ReadMethodArgs<'getMarkets'>;
        return (await this.adapter.getMarkets(this.requireAccount(opts))) as ReadMethodResult<M>;
      }
      case 'getMarket': {
        const [symbol, opts] = args as ReadMethodArgs<'getMarket'>;
        return (await this.adapter.getMarket(symbol, this.requireAccount(opts))) as ReadMethodResult<M>;
      }

      case 'getTicker': {
        const [symbol, opts] = args as ReadMethodArgs<'getTicker'>;
        return (await this.adapter.getTicker(symbol, this.requireAccount(opts))) as ReadMethodResult<M>;
      }
      case 'getTickers': {
        const [symbols, opts] = args as ReadMethodArgs<'getTickers'>;
        return (await this.adapter.getTickers(symbols, this.requireAccount(opts))) as ReadMethodResult<M>;
      }

      case 'getPositions': {
        const [opts] = args as ReadMethodArgs<'getPositions'>;
        return (await this.adapter.getPositions({
          account: this.requireAccount(opts),
          symbol: opts?.symbol,
        })) as ReadMethodResult<M>;
      }
      case 'getHistoricalPosition': {
        const [query] = args as ReadMethodArgs<'getHistoricalPosition'>;
        return (await this.adapter.getHistoricalPosition(query)) as ReadMethodResult<M>;
      }

      case 'getOrders': {
        const [opts] = args as ReadMethodArgs<'getOrders'>;
        return (await this.adapter.getOrders({
          account: this.requireAccount(opts),
          symbol: opts?.symbol,
        })) as ReadMethodResult<M>;
      }

      case 'getBalance': {
        const [opts] = args as ReadMethodArgs<'getBalance'>;
        return (await this.adapter.getBalance(this.requireAccount(opts))) as ReadMethodResult<M>;
      }
      case 'getCombinedBalance': {
        const [opts] = args as ReadMethodArgs<'getCombinedBalance'>;
        return (await this.adapter.getCombinedBalance({
          accounts: opts?.accounts,
        })) as ReadMethodResult<M>;
      }

      case 'getOrderbook': {
        const [symbol, opts] = args as ReadMethodArgs<'getOrderbook'>;
        return (await this.adapter.getOrderbook(symbol, {
          account: this.requireAccount(opts),
          depth: opts?.depth,
        })) as ReadMethodResult<M>;
      }

      case 'getCandles': {
        const [symbol, timeframe, opts] = args as ReadMethodArgs<'getCandles'>;
        return (await this.adapter.getCandles(symbol, timeframe, {
          account: this.requireAccount(opts),
          from: opts?.from,
          to: opts?.to,
          limit: opts?.limit,
        })) as ReadMethodResult<M>;
      }
    }

    // Exhaustiveness check — if a new method lands in `ReadMethodName`
    // without a case, this assignment fails at compile time.
    const _exhaustive: never = method;
    void _exhaustive;
    throw new Error(`InProcessProxy: unknown method '${String(method)}'`);
  }

  async callWrite<M extends WriteMethodName>(method: M, args: WriteMethodArgs<M>): Promise<WriteMethodResult<M>> {
    switch (method) {
      case 'placeOrder': {
        const [opts] = args as WriteMethodArgs<'placeOrder'>;
        const { account: _drop, ...rest } = opts;
        return (await this.adapter.placeOrder(rest, this.requireAccount(opts))) as WriteMethodResult<M>;
      }
      case 'placeOrders': {
        const [list] = args as WriteMethodArgs<'placeOrders'>;
        // Empty batch is a valid no-op — short-circuit before dereferencing
        // `list[0]` (which would crash `requireAccount(undefined)`).
        if (list.length === 0) {
          return { orderIds: [] } as WriteMethodResult<M>;
        }
        // Account is resolved per-call by the dispatcher; placeOrders takes a
        // batch that shares one resolved account. The dispatcher rewrites
        // each entry's `account` to the same resolved value, so reading
        // from any element is safe.
        const account = this.requireAccount(list[0]);
        const stripped = list.map(({ account: _drop, ...rest }) => rest);
        return (await this.adapter.placeOrders(stripped, account)) as WriteMethodResult<M>;
      }
      case 'cancelOrder': {
        const [orderId, opts] = args as WriteMethodArgs<'cancelOrder'>;
        const account = this.requireAccount(opts);
        return (await this.adapter.cancelOrder(orderId, { account, symbol: opts?.symbol })) as WriteMethodResult<M>;
      }
      case 'cancelOrders': {
        const [filter, opts] = args as WriteMethodArgs<'cancelOrders'>;
        return (await this.adapter.cancelOrders(filter, this.requireAccount(opts))) as WriteMethodResult<M>;
      }
      case 'updateOrder': {
        const [orderId, patch, opts] = args as WriteMethodArgs<'updateOrder'>;
        return (await this.adapter.updateOrder(orderId, patch, this.requireAccount(opts))) as WriteMethodResult<M>;
      }
      case 'closePosition': {
        const [opts] = args as WriteMethodArgs<'closePosition'>;
        const { account: _drop, ...rest } = opts;
        return (await this.adapter.closePosition(rest, this.requireAccount(opts))) as WriteMethodResult<M>;
      }
      case 'setLeverage': {
        const [symbol, leverage, opts] = args as WriteMethodArgs<'setLeverage'>;
        return (await this.adapter.setLeverage(symbol, leverage, this.requireAccount(opts))) as WriteMethodResult<M>;
      }
      case 'setPositionMode': {
        const [mode, opts] = args as WriteMethodArgs<'setPositionMode'>;
        return (await this.adapter.setPositionMode(mode, this.requireAccount(opts))) as WriteMethodResult<M>;
      }
      case 'setMarginType': {
        const [symbol, marginType, opts] = args as WriteMethodArgs<'setMarginType'>;
        return (await this.adapter.setMarginType(
          symbol,
          marginType,
          this.requireAccount(opts),
        )) as WriteMethodResult<M>;
      }
      case 'runCommand': {
        const [command, opts] = args as WriteMethodArgs<'runCommand'>;
        // runCommand depth threading is the dispatcher's job — see
        // `buildApi`'s exchange.runCommand path. The depth is carried in
        // this internal proxy envelope, not in the public script-facing
        // CoreApi opts type.
        const depth = opts?.depth ?? 1;
        return (await this.adapter.runCommand(command, {
          account: this.requireAccount(opts),
          symbol: opts?.symbol,
          depth,
        })) as WriteMethodResult<M>;
      }
    }

    const _exhaustive: never = method;
    void _exhaustive;
    throw new Error(`InProcessProxy: unknown write method '${String(method)}'`);
  }

  /**
   * Guard for adapter signatures that take an `account: string`. The
   * dispatcher (runtime/accountResolver.ts) is responsible for resolving
   * `opts.account` per SPEC §2.6 before reaching the proxy. This guard is a
   * belt-and-suspenders sanity check; misuse from inside core would throw
   * here rather than passing `undefined` to a host adapter.
   */
  private requireAccount(opts: { account?: string } | undefined): string {
    const account = opts?.account;
    if (!account) {
      throw new Error('InProcessProxy: account must be resolved before dispatch (see SPEC §2.6 accountResolver).');
    }
    return account;
  }
}
