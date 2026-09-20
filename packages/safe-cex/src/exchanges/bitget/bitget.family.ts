import type { ExchangeName } from '../../types';

/**
 * The set of exchanges that share Bitget's private-order shape, including the
 * synthetic `__stop_loss` / `__take_profit` virtual attached rows generated
 * by our order normalization.
 *
 * Consumers should use `isBitgetLikeExchange` rather than open-coding this
 * membership check. When a new Bitget variant is registered (for example a
 * hypothetical `bitgetuta`), it must be added here so bulk-cancel actions
 * and other Bitget-specific UI branches pick it up automatically instead of
 * silently regressing to the pre-fix behavior.
 */
export const BITGET_LIKE_EXCHANGES = new Set<ExchangeName>(['bitget', 'bitgetusdc', 'bitgetinverse']);

export const isBitgetLikeExchange = (exchange: ExchangeName | string | undefined): boolean =>
  typeof exchange === 'string' && BITGET_LIKE_EXCHANGES.has(exchange as ExchangeName);

/**
 * Bitget virtual attached TP/SL rows are synthesized on the client. Their id
 * is `${parentOrderId}__stop_loss` or `${parentOrderId}__take_profit`. Bitget's
 * own order ids are numeric strings and cannot collide with either suffix.
 *
 * This predicate is shared by any UI that needs to distinguish those synthetic
 * rows from real orders — bulk cancel-by-side is the canonical case, but any
 * future side-scoped or type-scoped action against a Bitget account should
 * consult it as well.
 */
export const isBitgetVirtualAttachedTpSlOrderId = (id?: string): boolean =>
  id?.endsWith('__stop_loss') === true || id?.endsWith('__take_profit') === true;

export const isBitgetVirtualAttachedTpSlOrder = (
  order: { id?: string },
  exchangeName: ExchangeName | string | undefined,
): boolean => isBitgetLikeExchange(exchangeName) && isBitgetVirtualAttachedTpSlOrderId(order.id);
