import type { Axios } from 'axios';
import type { BybitCreateOrderResponse } from './bybit.exchange';

import { nanoid } from 'nanoid';

import { getBybitResponseMeta } from './bybit.response';
import { ENDPOINTS } from '../../../exchanges/bybit/bybit.types';

export const hyroSubmissionKey = (value: unknown): string =>
  JSON.stringify(value, (_key, item) => {
    if (item && typeof item === 'object' && !Array.isArray(item)) {
      return Object.fromEntries(
        Object.entries(item)
          .filter(([key]) => key !== 'orderLinkId')
          .sort(([a], [b]) => a.localeCompare(b)),
      );
    }
    return item;
  });

class HyroOrderRejectedError extends Error {}

const isDuplicateIdentity = (code?: number, message = '') =>
  code === 10014 || code === 110072 || /duplicate|duplicated|already.*exist/i.test(message);

type PendingCreate = { payload: Record<string, any>; uncertain: boolean; accepted?: BybitCreateOrderResponse };
type ReadyCreate = { response: BybitCreateOrderResponse; boundary: string };
type PendingGroup = {
  creates: PendingCreate[];
  responses: BybitCreateOrderResponse[];
  observedLinkIds: Set<string>;
  running?: Promise<BybitCreateOrderResponse[]>;
};

export class HyroOrderRecovery {
  private pending = new Map<string, PendingGroup>();

  constructor(
    private readonly getClient: () => Axios,
    private readonly isDisposed: () => boolean,
    private readonly assertCanCreate: () => void,
    private readonly getPublicationBoundary: () => string,
  ) {}

  observeOrders(rows: Record<string, any>[]) {
    for (const row of rows) {
      if (typeof row.orderLinkId !== 'string') continue;
      for (const group of this.pending.values()) {
        if (group.creates.some((create) => create.payload.orderLinkId === row.orderLinkId)) {
          group.observedLinkIds.add(row.orderLinkId);
        }
      }
    }
  }

  dispose() {
    this.pending.clear();
  }

  async submit(payloads: Record<string, any>[], onAccepted: (response: BybitCreateOrderResponse) => void) {
    const key = hyroSubmissionKey(payloads);
    let group = this.pending.get(key);
    if (group?.running) return group.running;
    if (!group) {
      group = {
        creates: payloads.map((payload) => ({ payload, uncertain: false })),
        responses: [],
        observedLinkIds: new Set(),
      };
      this.pending.set(key, group);
    }
    const current = group;
    current.running = this.runGroup(current, onAccepted);
    try {
      const responses = await current.running;
      this.pending.delete(key);
      return responses;
    } catch (error) {
      if (error instanceof HyroOrderRejectedError) {
        const rejected = current.creates[current.responses.length];
        rejected.payload = { ...rejected.payload, orderLinkId: nanoid() };
        rejected.uncertain = false;
        if (current.responses.length === 0) this.pending.delete(key);
      }
      throw error;
    } finally {
      current.running = undefined;
    }
  }

  private async runGroup(group: PendingGroup, onAccepted: (response: BybitCreateOrderResponse) => void) {
    for (let index = group.responses.length; index < group.creates.length; index++) {
      const create = group.creates[index];
      const { response, boundary } = await this.submitOne(create);
      if (this.isDisposed() || boundary !== this.getPublicationBoundary()) {
        create.uncertain = true;
        throw this.unknown(
          create.payload,
          'private session or authoritative orders snapshot changed before publication',
        );
      }
      response.authoritativeSeen = group.observedLinkIds.has(response.payload.orderLinkId);
      group.responses.push(response);
      onAccepted(response);
    }
    return group.responses;
  }

  private unknown(payload: Record<string, any>, cause: unknown): Error {
    const reason = cause instanceof Error ? cause.message : String(cause);
    return new Error(
      `HyroTrader order outcome is unknown (orderLinkId ${payload.orderLinkId}). Reconcile before placing a replacement: ${reason}`,
    );
  }

  private async submitOne(create: PendingCreate): Promise<ReadyCreate> {
    const { payload } = create;
    let attempts = 0;
    let cause: unknown = 'submission could not be confirmed';
    while (!this.isDisposed()) {
      if (create.uncertain) {
        const boundary = this.getPublicationBoundary();
        let found: Record<string, any> | undefined;
        try {
          found = await this.findOrder(payload);
        } catch (error) {
          throw this.unknown(payload, error);
        }
        if (found) {
          if (create.accepted && found.orderId !== create.accepted.data.result.orderId) {
            throw this.unknown(payload, 'reconciled identity differs from the acknowledged order');
          }
          if (found.orderStatus !== 'Rejected') {
            create.accepted = { payload, data: { retCode: 0, retMsg: 'OK', result: { orderId: found.orderId } } };
          }
        }
        if (boundary !== this.getPublicationBoundary()) {
          throw this.unknown(payload, 'private session or authoritative orders snapshot changed during reconciliation');
        }
        if (found) {
          if (found.orderStatus === 'Rejected') {
            throw new HyroOrderRejectedError(`HyroTrader order rejected: ${found.rejectReason || payload.orderLinkId}`);
          }
          return {
            boundary,
            response: {
              payload,
              recoveredOrder: found,
              data: { retCode: 0, retMsg: 'OK', result: { orderId: found.orderId } },
              terminal: ['Filled', 'Cancelled', 'Deactivated', 'PartiallyFilledCanceled'].includes(found.orderStatus),
            },
          };
        }
        if (create.accepted) return { response: { ...create.accepted, reconciled: true }, boundary };
        if (attempts >= 2) throw this.unknown(payload, cause);
        create.uncertain = false;
      }
      if (this.isDisposed()) break;
      this.assertCanCreate();
      attempts++;
      const boundary = this.getPublicationBoundary();
      try {
        const { data, status: httpStatus } = await this.getClient().post(ENDPOINTS.CREATE_ORDER, payload, {
          'axios-retry': { retries: 0 },
        });
        const status = getBybitResponseMeta(data);
        if (status.isSuccess && typeof data?.result?.orderId === 'string' && data.result.orderId) {
          create.accepted = { data, payload };
          if (boundary === this.getPublicationBoundary()) return { response: create.accepted, boundary };
          create.uncertain = true;
          cause = 'private session or authoritative orders snapshot changed before acknowledgement';
          continue;
        }
        const ambiguous =
          isDuplicateIdentity(status.retCode, status.retMsg) ||
          httpStatus === 409 ||
          httpStatus === 408 ||
          (httpStatus !== undefined && httpStatus >= 500) ||
          status.retCode === 408 ||
          (status.retCode !== undefined && status.retCode >= 500 && status.retCode < 600) ||
          status.retCode === 10000 ||
          status.retCode === 10016;
        if (!status.isSuccess && status.retCode !== undefined && !ambiguous) {
          throw new HyroOrderRejectedError(status.retMsg || `HyroTrader order rejected (${status.retCode})`);
        }
        cause = status.retMsg || 'Malformed create response';
      } catch (error) {
        if (error instanceof HyroOrderRejectedError) throw error;
        const response = (error as { response?: { status?: number; data?: unknown } })?.response;
        const status = response?.status;
        const meta = getBybitResponseMeta(response?.data);
        if (
          status !== undefined &&
          status >= 400 &&
          status < 500 &&
          status !== 408 &&
          status !== 409 &&
          !isDuplicateIdentity(meta.retCode, meta.retMsg)
        ) {
          throw new HyroOrderRejectedError(
            error instanceof Error ? error.message : `HyroTrader request rejected (${status})`,
          );
        }
        cause = error;
      }
      create.uncertain = true;
    }
    throw this.unknown(payload, 'exchange was disposed during submission');
  }

  private async findOrder(payload: Record<string, any>) {
    for (const endpoint of [ENDPOINTS.UNFILLED_ORDERS, ENDPOINTS.USER_ORDERS]) {
      let cursor = '';
      const seen = new Set<string>();
      for (let page = 0; page < 100; page++) {
        if (this.isDisposed()) throw new Error('exchange was disposed during reconciliation');
        const { data } = await this.getClient().get(endpoint, {
          params: { category: 'linear', symbol: payload.symbol, limit: 50, cursor },
        });
        const status = getBybitResponseMeta(data);
        if (!status.isSuccess || !Array.isArray(data?.result?.list)) {
          throw new Error(
            status.retMsg && !status.isSuccess ? status.retMsg : 'Malformed order reconciliation response',
          );
        }
        const found = data.result.list.find((row: Record<string, any>) => row.orderLinkId === payload.orderLinkId);
        if (found) {
          if (
            typeof found.orderId !== 'string' ||
            !found.orderId ||
            ![
              'New',
              'Created',
              'Untriggered',
              'Triggered',
              'PartiallyFilled',
              'Filled',
              'Cancelled',
              'Rejected',
              'Deactivated',
              'PartiallyFilledCanceled',
            ].includes(found.orderStatus)
          )
            throw new Error('Matched order has no identity or status');
          return found as Record<string, any>;
        }
        const next = data.result.nextPageCursor;
        if (next === undefined || next === '') break;
        if (typeof next !== 'string' || seen.has(next)) throw new Error('Order reconciliation cursor did not advance');
        seen.add(next);
        cursor = next;
        if (page === 99) throw new Error('Order reconciliation page limit exceeded');
      }
    }
    return undefined;
  }
}
