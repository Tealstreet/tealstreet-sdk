import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { OrderFillEvent } from '../generated/OrderFillEvent';

import { OrderSide } from '../types';

/** Decode native identities and NaN optional-column sentinels, without repairing events. */
export function readFillFrame(frame: Frame): OrderFillEvent[] {
  const rows = frame.fills();
  const fills: OrderFillEvent[] = [];
  if (!rows) return fills;
  for (let i = 0; i < rows.count; i++) {
    const id = rows.fillId?.(i);
    if (id === undefined) throw new Error('Fill identity requires a native event id');
    const symbol = rows.symbol?.(i);
    if (symbol === undefined) throw new Error('Fill identity requires a native symbol');
    const side = rows.side[i] === 0 ? OrderSide.Buy : rows.side[i] === 1 ? OrderSide.Sell : undefined;
    if (side === undefined) throw new Error('Invalid Fill side');
    const realizedPnl = rows.realizedPnl?.[i];
    const fee = rows.fee?.[i];
    if (
      (realizedPnl !== undefined && !Number.isNaN(realizedPnl) && !Number.isFinite(realizedPnl)) ||
      (fee !== undefined && !Number.isNaN(fee) && !Number.isFinite(fee))
    ) {
      throw new Error('Invalid Fill financial column');
    }
    fills.push({
      id,
      orderId: rows.orderId(i),
      symbol,
      side,
      price: rows.rows[i * 2],
      amount: rows.rows[i * 2 + 1],
      ...(realizedPnl !== undefined && !Number.isNaN(realizedPnl) && { realizedPnl }),
      ...(fee !== undefined && !Number.isNaN(fee) && { fee }),
    });
  }
  return fills;
}
