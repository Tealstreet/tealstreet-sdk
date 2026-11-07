import type { Order, PlaceOrderOpts, UpdateOrderOpts } from '../../../types';
import type { TxFlowExchange } from './txflow.exchange';

import { OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../../types';
import { virtualClock } from '../../../utils/virtualClock';
import { signAction } from './txflow.signing';

export { txflowClientOrderIdRule, validTxFlowCloid } from '../../../exchanges/txflow/txflow.client-id';
import { validTxFlowCloid } from '../../../exchanges/txflow/txflow.client-id';
import { uuid } from '../../../utils/uuid';

type OrderWire = { a: number; b: boolean; p: string; s: string; r: boolean; t: { limit: { tif: string } }; c: string };
type OrderAction = { grouping: 'na'; orders: OrderWire[] };
type CancelAction = { cancels: { a: number; o: number }[] };
type ModifyAction = { order: { o: number; p: string; s: string } };
type Action =
  | OrderAction
  | CancelAction
  | ModifyAction
  | { asset: number; leverage: number }
  | { asset: number; isCross: boolean }
  | { isOneway: boolean };
type Reply = {
  status: string;
  confirm?: string;
  code?: number;
  message?: string;
  response?: string;
  data?: { type: string; data: { statuses: (string | { error: string })[] } };
};

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

const numberWire = (value: number, step: number): string => {
  if (
    !Number.isFinite(value) ||
    value <= 0 ||
    !Number.isFinite(step) ||
    step <= 0 ||
    Math.abs(Math.round(value / step) * step - value) > step * 1e-7
  ) {
    throw new Error('TxFlow order number does not match market precision');
  }
  const text = value.toFixed(12);
  if (Number(text) !== value) throw new Error('TxFlow order number exceeds supported precision');
  return text.replace(/\.?0+$/, '');
};

/** Test-only replay reference; production orders use the Rust venue. */
export class TxFlowTrading {
  private tail: Promise<unknown> = Promise.resolve();
  private nonce = 0;
  private lastWrite = -Infinity;
  private reserved = new Set<string>();

  constructor(private exchange: TxFlowExchange) {}

  private serial<T>(job: () => Promise<T>): Promise<T> {
    const next = this.tail.then(job);
    this.tail = next.catch(() => undefined);
    return next;
  }

  private async execute(type: string, action: Action, responseType: string, prepare?: () => Action): Promise<void> {
    const { exchange } = this;
    if (exchange.isDisposed) exchange.reject('TxFlow exchange is disposed');
    if (!exchange.options.testnet || exchange.options.publicDataOnly || !exchange.wallet())
      exchange.reject('TxFlow trading requires an authenticated testnet account');
    const elapsed = virtualClock.getCurrentTimeMs() - this.lastWrite;
    if (elapsed < 110) await delay(110 - elapsed);
    if (type === 'modify2') {
      const id = String((action as ModifyAction).order.o);
      const current = exchange.store.orders.find((o) => o.id === id);
      const native = current?.info as { childrenKnown?: boolean } | undefined;
      if (
        !current ||
        current.status !== OrderStatus.Open ||
        !native?.childrenKnown ||
        current.takeProfit !== undefined ||
        current.stopLoss !== undefined ||
        exchange.store.orders.some((o) => o.parentId === id)
      )
        exchange.reject('TxFlow modify2 cannot clear unverified TP/SL children');
    }
    if (type === 'updateLeverage' && exchange.store.options.isHedged)
      exchange.reject('TxFlow hedge leverage requires an explicit side');
    if (
      type === 'updatePositionMode' &&
      (exchange.store.orders.length || exchange.store.positions.some((p) => p.contracts !== 0))
    )
      exchange.reject('TxFlow position mode requires a flat account without orders');
    if (prepare) action = prepare();
    if (exchange.isDisposed) exchange.reject('TxFlow exchange is disposed');
    const now = virtualClock.getCurrentTimeMs();
    if (!Number.isFinite(now) || now < 0 || now > Number.MAX_SAFE_INTEGER)
      exchange.reject('Invalid TxFlow signing time');
    const nonce = Math.max(Math.floor(now), this.nonce + 1);
    if (!Number.isSafeInteger(nonce)) exchange.reject('TxFlow nonce exhausted');
    const signature = await signAction(action, nonce, exchange.options.secret ?? '', exchange.options.vault);
    if (prepare && JSON.stringify(prepare()) !== JSON.stringify(action))
      exchange.reject('TxFlow order changed during signing; refresh before retry');
    if (exchange.isDisposed) exchange.reject('TxFlow exchange is disposed');
    this.nonce = nonce;
    this.lastWrite = now;
    try {
      const response = await exchange.xhr.post<Reply>(
        '/exchange',
        {
          action: { type, ...action },
          signature,
          nonce,
          ...(exchange.options.vault ? { vaultAddress: exchange.options.vault } : {}),
          confirm: 'executed',
        },
        { headers: { 'Content-Type': 'application/json', 'x-api-version': '2026-08-16' } },
      );
      const reply = response.data;
      if (response.status !== 200) throw new Error(`TxFlow HTTP ${response.status} execution outcome is uncertain`);
      if (reply.status === 'err') throw new Error(reply.message ?? reply.response ?? 'TxFlow action rejected');
      if (reply.status !== 'ok' || reply.code !== 0 || reply.confirm !== 'executed' || !reply.data)
        throw new Error('TxFlow execution outcome is uncertain');
      if (reply.data.type !== responseType || reply.data.data.statuses.length !== 1)
        throw new Error('Invalid TxFlow whole-action acknowledgement');
      const status = reply.data.data.statuses[0];
      if (status !== 'success')
        throw new Error(typeof status === 'object' ? status.error : 'Unknown TxFlow execution status');
    } catch (error) {
      exchange.account.writeAccepted();
      throw error;
    }
    exchange.account.writeAccepted();
  }

  placeOrders = (orders: PlaceOrderOpts[]): Promise<string[]> => {
    const seen = new Set<string>();
    // Build the whole command synchronously before reserving IDs or issuing writes.
    const wires: OrderWire[] = orders.map((opts) => {
      const market = this.exchange.store.marketsBySymbol[opts.symbol];
      if (!market) this.exchange.reject('Unknown TxFlow market');
      if (!market.active) this.exchange.reject('TxFlow market is halted');
      if (
        opts.stopLoss !== undefined ||
        opts.takeProfit !== undefined ||
        opts.reduceOnly ||
        opts.close ||
        ![OrderType.Limit, OrderType.Market].includes(opts.type) ||
        opts.triggerPrice !== undefined ||
        opts.triggerType !== undefined ||
        opts.tpTriggerType !== undefined ||
        opts.slTriggerType !== undefined
      ) {
        this.exchange.reject('TxFlow protective, conditional and reduce-only semantics require testnet measurement');
      }
      if (
        opts.impliedVolatility !== undefined ||
        Object.hasOwn(opts, 'stpMode') ||
        Object.hasOwn(opts, 'bybitBbo') ||
        Object.hasOwn(opts, 'bybitSlippageToleranceTicks')
      ) {
        this.exchange.reject('TxFlow does not accept venue-specific execution controls');
      }
      if (opts.maxSlippagePct !== undefined && (opts.type !== OrderType.Market || opts.maxSlippagePct !== 5))
        this.exchange.reject('TxFlow market slippage is fixed at 5 percent');
      let tif: string;
      if (opts.type === OrderType.Market) {
        if (opts.timeInForce !== undefined && opts.timeInForce !== OrderTimeInForce.ImmediateOrCancel)
          this.exchange.reject('TxFlow market orders use immediate execution');
        if (opts.price !== undefined) this.exchange.reject('TxFlow market orders do not accept a limit price');
        tif = 'FrontendMarket';
      } else {
        if (opts.timeInForce === OrderTimeInForce.FillOrKill)
          this.exchange.reject('TxFlow does not document fill-or-kill');
        tif =
          opts.timeInForce === OrderTimeInForce.PostOnly
            ? 'Alo'
            : opts.timeInForce === OrderTimeInForce.ImmediateOrCancel
              ? 'Ioc'
              : 'Gtc';
      }
      const raw = market.info as {
        useNotionalMargin?: boolean;
        maxMarketOrderSize?: number;
        maxLimitOrderSize?: number;
      };
      if (raw?.useNotionalMargin) this.exchange.reject('TxFlow notional-margin semantics require testnet measurement');
      const max = opts.type === OrderType.Market ? raw?.maxMarketOrderSize : raw?.maxLimitOrderSize;
      if (
        opts.amount < market.limits.amount.min ||
        opts.amount > market.limits.amount.max ||
        (max != null && opts.amount > max)
      )
        this.exchange.reject('TxFlow order size exceeds market limits');
      const cloid = opts.clientOrderId ?? `ts_${uuid()}`;
      if (typeof cloid !== 'string' || !validTxFlowCloid(cloid))
        this.exchange.reject('TxFlow clientOrderId must be nonempty, at most 64 UTF-8 bytes and not start with 0x');
      if (
        seen.has(cloid) ||
        this.reserved.has(cloid) ||
        this.exchange.store.orders.some((o) => o.clientOrderId === cloid)
      )
        this.exchange.reject('Duplicate TxFlow clientOrderId');
      seen.add(cloid);
      return {
        a: this.asset(opts.symbol),
        b: opts.side === OrderSide.Buy,
        p: opts.type === OrderType.Market ? 'bbo' : numberWire(opts.price ?? NaN, market.precision.price),
        s: numberWire(opts.amount, market.precision.amount),
        r: false,
        t: { limit: { tif } },
        c: cloid,
      };
    });
    seen.forEach((id) => this.reserved.add(id));
    return this.serial(async () => {
      const ids: string[] = [];
      for (let index = 0; index < wires.length; index += 50) {
        const batch = wires.slice(index, index + 50);
        const previous = new Map(batch.map((o) => [o.c, this.exchange.account.orderIds.get(o.c)]));
        await this.execute('order', { grouping: 'na', orders: batch }, 'PlaceOrder');
        for (let attempt = 0; attempt < 8; attempt += 1) {
          const resolved = batch.map((o) => {
            const id = this.exchange.account.orderIds.get(o.c);
            return id !== previous.get(o.c) ? id : undefined;
          });
          if (resolved.every((id): id is string => !!id)) {
            ids.push(...resolved);
            break;
          }
          await delay(300);
          await this.exchange.account.refreshOrders();
          if (attempt === 7)
            throw new Error(
              'TxFlow executed placement has unresolved identity; reconcile by clientOrderId before resubmitting',
            );
        }
      }
      return ids;
    }).finally(() => seen.forEach((id) => this.reserved.delete(id)));
  };

  cancelOrders = (orders: Order[]): Promise<void> => {
    const cancels = orders.map((order) => {
      if (![OrderType.Limit, OrderType.Market].includes(order.type))
        this.exchange.reject('TxFlow grouped cancellation requires native grouping metadata');
      return { a: this.asset(order.symbol), o: this.oid(order.id) };
    });
    return this.serial(async () => {
      for (let index = 0; index < cancels.length; index += 200)
        await this.execute('cancel', { cancels: cancels.slice(index, index + 200) }, 'CancelOrder');
    });
  };
  cancelSymbolOrders = (symbol: string): Promise<void> =>
    this.cancelOrders(this.exchange.store.orders.filter((o) => o.symbol === symbol));

  updateOrder = ({ order, update }: UpdateOrderOpts): Promise<string[]> => {
    this.prepareAmend(order.id, update);
    return this.serial(async () => {
      const action = this.prepareAmend(order.id, update);
      await this.execute('modify2', action, 'Modify2', () => this.prepareAmend(order.id, update));
      return [order.id];
    });
  };
  private prepareAmend(id: string, update: UpdateOrderOpts['update']): ModifyAction {
    const current =
      this.exchange.store.orders.find((o) => o.id === id) ??
      this.exchange.reject('TxFlow modify2 requires a known resting order');
    if (current.status !== OrderStatus.Open || current.type !== OrderType.Limit)
      this.exchange.reject('TxFlow modify2 requires a resting limit order');
    const native = current.info as { children?: unknown[] | null; childrenKnown?: boolean } | undefined;
    if (
      current.takeProfit !== undefined ||
      current.stopLoss !== undefined ||
      !native?.childrenKnown ||
      native.children?.length ||
      this.exchange.store.orders.some((o) => o.parentId === current.id)
    )
      this.exchange.reject('TxFlow modify2 cannot clear unverified TP/SL children');
    const market = this.exchange.store.marketsBySymbol[current.symbol];
    const amount = 'amount' in update ? update.amount : current.amount;
    if (!market?.active || amount < market.limits.amount.min || amount > market.limits.amount.max)
      this.exchange.reject('Invalid TxFlow amend size or halted market');
    return {
      order: {
        o: this.oid(current.id),
        p: numberWire('price' in update ? update.price : current.price, market.precision.price),
        s: numberWire(amount, market.precision.amount),
      },
    };
  }
  setLeverage = (symbol: string, leverage: number): Promise<void> => {
    const market = this.exchange.store.marketsBySymbol[symbol];
    if (
      !market ||
      !Number.isInteger(leverage) ||
      leverage < 1 ||
      leverage > market.limits.leverage.max ||
      leverage > 2147483647
    )
      this.exchange.reject('Invalid TxFlow leverage');
    if (this.exchange.store.options.isHedged) this.exchange.reject('TxFlow hedge leverage requires an explicit side');
    const asset = this.asset(symbol);
    return this.serial(() => {
      if (this.exchange.store.options.isHedged) this.exchange.reject('TxFlow hedge leverage requires an explicit side');
      return this.execute('updateLeverage', { asset, leverage }, 'UpdateLeverage');
    });
  };
  changeMarginType = (isolated: boolean, symbol?: string): Promise<void> => {
    const asset = this.asset(symbol ?? '');
    return this.serial(() => this.execute('updateMarginMode', { asset, isCross: !isolated }, 'UpdateMarginMode'));
  };
  changePositionMode = (hedged: boolean): Promise<void> => {
    if (this.exchange.store.positions.some((p) => p.contracts !== 0) || this.exchange.store.orders.length)
      this.exchange.reject('TxFlow position mode requires a flat account without orders');
    return this.serial(async () => {
      if (this.exchange.store.positions.some((p) => p.contracts !== 0) || this.exchange.store.orders.length)
        this.exchange.reject('TxFlow position mode requires a flat account without orders');
      await this.execute('updatePositionMode', { isOneway: !hedged }, 'UpdatePositionMode');
      this.exchange.store.setSetting('isHedged', hedged);
    });
  };
  private asset(symbol: string): number {
    const market = this.exchange.store.marketsBySymbol[symbol];
    const id = Number(market?.id);
    if (!market || !Number.isInteger(id) || id < 0 || id > 4294967295) this.exchange.reject('Unknown TxFlow asset');
    return id;
  }
  private oid(id: string): number {
    const value = Number(id);
    if (!/^\d+$/.test(id) || !Number.isSafeInteger(value)) this.exchange.reject('Invalid TxFlow order ID');
    return value;
  }
}
