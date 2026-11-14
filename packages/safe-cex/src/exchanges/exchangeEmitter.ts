import { TinyEmitter } from 'tiny-emitter';

import { diagnosticConsole, redactDiagnosticData } from '../utils/diagnosticRedaction';

type Listener = { fn: (...args: unknown[]) => unknown; ctx: unknown };

export class ExchangeEmitter extends TinyEmitter {
  constructor(private readonly exchange: string) {
    super();
  }

  emit(event: string, ...args: unknown[]): this {
    const diagnostic =
      event === 'log' ||
      event === 'exec-log' ||
      event === 'error' ||
      event === 'warning' ||
      event === 'engine-event' ||
      event === 'ws-diagnostic' ||
      event.startsWith('grafana-');
    const delivered = diagnostic ? args.map((arg) => redactDiagnosticData(arg)) : args;
    for (const listener of this.listeners(event)) {
      try {
        listener.fn.apply(listener.ctx, delivered);
      } catch (cause) {
        diagnosticConsole.error(`[${this.exchange}] Event "${event}" listener failed`, cause);
      }
    }
    return this;
  }

  /** Like emit, but rethrows the first listener failure after every listener ran, for
   * transport dispatch whose caller must learn that its request was never sent. */
  dispatch(event: string, ...args: unknown[]): void {
    let failure: { cause: unknown } | undefined;
    for (const listener of this.listeners(event)) {
      try {
        listener.fn.apply(listener.ctx, args);
      } catch (cause) {
        failure ??= { cause };
      }
    }
    if (failure) throw failure.cause;
  }

  hasListeners(event: string): boolean {
    return this.listeners(event).length > 0;
  }

  /** Capture subscription identity across an await, including off/on of the same callback. */
  captureListenerGuard(event: string): () => boolean {
    const captured = this.listeners(event);
    return () => {
      const current = this.listeners(event);
      return current.length === captured.length && current.every((listener, index) => listener === captured[index]);
    };
  }

  private listeners(event: string): Listener[] {
    // TinyEmitter's inherited on/once/off own these records; snapshot before
    // dispatch so subscription changes retain its current-emission semantics.
    const events = (this as TinyEmitter & { e?: Record<string, Listener[]> }).e;
    return events?.[event]?.slice() ?? [];
  }
}
