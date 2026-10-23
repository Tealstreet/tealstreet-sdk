/** Test isolation only: real endpoints never supply unit-test fixture data. */
export class UnitTransportGuard {
  private attempts: string[] = [];
  private firstAttempt: Error | undefined;
  private loopbackLeases = new Set<{ port: number }>();

  deny(transport: string): never {
    this.attempts.push(transport);
    const error = new Error(`Unstubbed unit-test transport: ${transport}`);
    this.firstAttempt ??= error;
    throw error;
  }

  assertClean(): void {
    if (this.attempts.length) {
      throw new Error(
        `Unstubbed unit-test transport (${this.attempts.length} attempts): ${[...new Set(this.attempts)].join(', ')}`,
        { cause: this.firstAttempt },
      );
    }
  }

  reset(): void {
    this.attempts = [];
    this.firstAttempt = undefined;
    this.loopbackLeases.clear();
  }

  /** Only a test's already-bound in-process server may receive real sockets. */
  allowLoopbackPort(port: number): () => void {
    if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Invalid fixture port');
    const lease = { port };
    this.loopbackLeases.add(lease);
    return () => this.loopbackLeases.delete(lease);
  }

  connect<T>(args: unknown[], delegate: () => T): T {
    // net.createConnection passes its normalized [options, listener] tuple;
    // direct Socket.connect also accepts options or (port, host, listener).
    const normalized = Array.isArray(args[0]) ? args[0] : args;
    const first = normalized[0];
    const options =
      typeof first === 'object' && first !== null
        ? (first as { port?: unknown; host?: unknown; path?: unknown })
        : { port: first, host: normalized[1], path: undefined };
    const port =
      typeof options.port === 'number'
        ? options.port
        : typeof options.port === 'string' && /^\d+$/.test(options.port)
          ? Number(options.port)
          : NaN;
    if (
      options.path === undefined &&
      (options.host === '127.0.0.1' || options.host === '::1') &&
      [...this.loopbackLeases].some((lease) => lease.port === port)
    )
      return delegate();
    return this.deny('socket.connect');
  }
}

export const unitTransportGuard = new UnitTransportGuard();
