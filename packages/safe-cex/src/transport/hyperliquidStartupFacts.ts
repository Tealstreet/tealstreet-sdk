type Read = { sent: boolean; waitingAt?: number };
type Route = 'direct' | 'how' | 'proxy';

/** Per-account counters only. Finished requests leave no retained request or account data. */
export class HyperliquidStartupFacts {
  private readonly reads = new Set<Read>();
  private waitTotal = 0;
  private waitMax = 0;
  private http429 = 0;
  private route: Route | 'mixed' | 'unknown' = 'unknown';
  private timeoutMs = 0;

  begin(route: Route, timeoutMs: number) {
    const read: Read = { sent: false };
    this.reads.add(read);
    this.route = this.route === 'unknown' || this.route === route ? route : 'mixed';
    this.timeoutMs = timeoutMs;
    return {
      close: () => this.reads.delete(read),
      sent: () => {
        read.sent = true;
      },
      response: (status: number) => {
        if (status === 429) this.http429 += 1;
      },
      wait: async <T>(work: () => Promise<T>): Promise<T> => {
        read.waitingAt = Date.now();
        try {
          return await work();
        } finally {
          const elapsed = Math.max(0, Date.now() - read.waitingAt);
          this.waitTotal += elapsed;
          this.waitMax = Math.max(this.waitMax, elapsed);
          read.waitingAt = undefined;
        }
      },
    };
  }

  tags(): Record<string, string> {
    let pending = 0;
    let inFlight = 0;
    let waiters = 0;
    let total = this.waitTotal;
    let maximum = this.waitMax;
    for (const read of this.reads) {
      if (read.sent) inFlight += 1;
      else pending += 1;
      if (read.waitingAt !== undefined) {
        waiters += 1;
        const elapsed = Math.max(0, Date.now() - read.waitingAt);
        total += elapsed;
        maximum = Math.max(maximum, elapsed);
      }
    }
    const count = (value: number) =>
      String(Number.isFinite(value) ? Math.min(Number.MAX_SAFE_INTEGER, Math.max(0, Math.floor(value))) : 0);
    return {
      startup_info_route: this.route,
      startup_info_http_429_count: count(this.http429),
      startup_info_read_timeout_ms: count(this.timeoutMs),
      startup_info_budget_wait_ms: count(total),
      startup_info_budget_max_wait_ms: count(maximum),
      startup_info_budget_waiters: count(waiters),
      startup_info_undispatched: count(pending),
      startup_info_in_flight: count(inFlight),
    };
  }
}
