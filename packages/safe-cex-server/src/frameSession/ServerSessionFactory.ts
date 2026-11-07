import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

import type { PublicExchangeHandle } from '../publicExchange/PublicExchangeFactory';
import { NativeFrameSession, type NativeFrameSessionOptions } from './NativeFrameSession';

export type NativeAccountRequest = {
  readonly exchange: ExchangeName;
  readonly accountId: string;
  readonly signal?: AbortSignal;
};

export type NativeAccountTarget = Pick<NativeFrameSessionOptions, 'url' | 'token'>;

export type NativeServerSessionRequest = NativeAccountRequest & {
  readonly engine: 'rust';
  readonly sink: NativeFrameSessionOptions['sink'];
};

export type TypescriptServerSessionRequest = {
  readonly engine: 'typescript';
  readonly exchange: ExchangeName;
};

export type ServerSessionFactoryOptions = {
  /** Owned by the caller; startup, cache policy and disposal remain there. */
  readonly publicExchangeFactory?: {
    get(exchange: ExchangeName): Promise<PublicExchangeHandle>;
  };
  /** Trusted application authorization binds this account AND venue to its target. */
  readonly resolveNativeAccount: (request: NativeAccountRequest) => Promise<NativeAccountTarget | null>;
  readonly nativeOptions?: Omit<NativeFrameSessionOptions, 'url' | 'token' | 'sink' | 'signal'>;
  readonly resolveTimeoutMs?: number;
};

/** Session selection is explicit; failed native connections never start a TS engine. */
export class ServerSessionFactory {
  constructor(private readonly options: ServerSessionFactoryOptions) {
    const timeout = options.resolveTimeoutMs ?? 10_000;
    if (!Number.isSafeInteger(timeout) || timeout <= 0 || timeout > 0x7fffffff) {
      throw new Error('Invalid native account resolution timeout');
    }
  }

  open(request: NativeServerSessionRequest): Promise<NativeFrameSession>;
  open(request: TypescriptServerSessionRequest): Promise<PublicExchangeHandle>;
  async open(
    request: NativeServerSessionRequest | TypescriptServerSessionRequest,
  ): Promise<NativeFrameSession | PublicExchangeHandle> {
    if (request.engine === 'typescript') {
      if (!this.options.publicExchangeFactory) throw new Error('TypeScript public exchange factory is not configured');
      return this.options.publicExchangeFactory.get(request.exchange);
    }
    if (request.engine !== 'rust') throw new Error('Explicit server session engine selection is required');
    if (typeof request.exchange !== 'string') throw new Error('Invalid native venue');
    const { isRustVenue } = await import('@tealstreet/safe-cex/src/exchanges/rust/rust.exchange');
    if (!isRustVenue(request.exchange)) throw new Error('Venue does not support native frame sessions');
    if (typeof request.accountId !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(request.accountId)) {
      throw new Error('Invalid native account identifier');
    }
    const target = await this.resolveTarget(request);
    if (!target) throw new Error('Native account is unavailable');
    return NativeFrameSession.connect({
      ...this.options.nativeOptions,
      url: target.url,
      token: target.token,
      sink: request.sink,
      signal: request.signal,
    });
  }

  private async resolveTarget(request: NativeAccountRequest): Promise<NativeAccountTarget | null> {
    if (request.signal?.aborted) throw new Error('Native account resolution failed');
    const lifetime = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let abort: (() => void) | undefined;
    try {
      const canceled = new Promise<never>((_, reject) => {
        abort = () => {
          lifetime.abort();
          reject(new Error('Native account resolution failed'));
        };
        timer = setTimeout(abort, this.options.resolveTimeoutMs ?? 10_000);
        request.signal?.addEventListener('abort', abort, { once: true });
      });
      return await Promise.race([
        Promise.resolve().then(() =>
          this.options.resolveNativeAccount({
            exchange: request.exchange,
            accountId: request.accountId,
            signal: lifetime.signal,
          }),
        ),
        canceled,
      ]);
    } catch {
      throw new Error('Native account resolution failed');
    } finally {
      clearTimeout(timer);
      if (abort) request.signal?.removeEventListener('abort', abort);
    }
  }
}
