import { TinyEmitter } from 'tiny-emitter';

type Listener = { fn: (...args: unknown[]) => unknown; ctx: unknown };

export class ExchangeEmitter extends TinyEmitter {
  constructor(private readonly exchange: string) {
    super();
  }

  emit(event: string, ...args: unknown[]): this {
    // TinyEmitter's inherited on/once/off own these records; snapshot before
    // dispatch so subscription changes retain its current-emission semantics.
    const events = (this as TinyEmitter & { e?: Record<string, Listener[]> }).e;
    const listeners = events?.[event]?.slice() ?? [];
    for (const listener of listeners) {
      try {
        listener.fn.apply(listener.ctx, args);
      } catch (cause) {
        console.error(`[${this.exchange}] Event "${event}" listener failed`, cause);
      }
    }
    return this;
  }
}
