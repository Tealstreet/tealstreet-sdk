export type CloseInitiator = 'local' | 'remote' | 'unknown';
export type ConnectionPhase = 'pre-open' | 'handshake' | 'established';
export type ConnectionRoute = 'direct' | 'relay' | 'HOW' | 'proxy';
export type ConnectionPort = '443' | '8443' | 'other' | 'unknown';
export type LifetimeBucket = '<5s' | '5-15s' | '15-30s' | '30-120s' | '>=120s' | 'unknown';
export type TimerLatenessBucket = '<1s' | '1-5s' | '5-20s' | '20-60s' | '>=60s' | 'unknown';

export interface ConnectionCloseDiagnostic {
  initiator: CloseInitiator;
  phase: ConnectionPhase;
  route: ConnectionRoute;
  port: ConnectionPort;
  lifetime: LifetimeBucket;
}
export interface AccountReconnectDiagnostics {
  public?: ConnectionCloseDiagnostic;
  private?: ConnectionCloseDiagnostic;
  business?: ConnectionCloseDiagnostic;
  how?: ConnectionCloseDiagnostic;
  hiddenTimerLateness: TimerLatenessBucket;
}

// Dedicated workers receive this from their page; Node hosts leave it unknown.
let workerVisibility: 'hidden' | 'visible' | undefined;
export const setConnectionVisibility = (value: 'hidden' | 'visible'): void => {
  workerVisibility = value;
};
const hidden = (): boolean => {
  const page = (globalThis as { document?: { visibilityState?: string } }).document;
  return (page?.visibilityState ?? workerVisibility) === 'hidden';
};
export const connectionPort = (url: string): ConnectionPort => {
  try {
    const parsed = new URL(url);
    const port = parsed.port || (parsed.protocol === 'wss:' ? '443' : '80');
    return port === '443' || port === '8443' ? port : 'other';
  } catch {
    return 'unknown';
  }
};
const lifetime = (ms?: number): LifetimeBucket => {
  if (ms === undefined || !Number.isFinite(ms) || ms < 0) return 'unknown';
  return ms < 5_000 ? '<5s' : ms < 15_000 ? '5-15s' : ms < 30_000 ? '15-30s' : ms < 120_000 ? '30-120s' : '>=120s';
};

export class HiddenTimerLateness {
  private maximum?: number;
  observe(dueAt: number): void {
    if (!hidden()) return;
    this.maximum = Math.max(this.maximum ?? 0, Math.max(0, Date.now() - dueAt));
  }
  get bucket(): TimerLatenessBucket {
    const ms = this.maximum;
    return ms === undefined
      ? 'unknown'
      : ms < 1_000
        ? '<1s'
        : ms < 5_000
          ? '1-5s'
          : ms < 20_000
            ? '5-20s'
            : ms < 60_000
              ? '20-60s'
              : '>=60s';
  }
}

export class ConnectionDiagnostics {
  private phase: ConnectionPhase = 'pre-open';
  private openedAt?: number;
  private port: ConnectionPort = 'unknown';
  private last?: ConnectionCloseDiagnostic;
  constructor(private readonly route: ConnectionRoute) {}
  dial(url: string): void {
    this.phase = 'pre-open';
    this.openedAt = undefined;
    this.port = connectionPort(url);
  }
  opened(handshake = false): void {
    this.openedAt = Date.now();
    this.phase = handshake ? 'handshake' : 'established';
  }
  established(): void {
    this.phase = 'established';
  }
  closed(initiator: CloseInitiator): ConnectionCloseDiagnostic {
    this.last = {
      initiator,
      phase: this.phase,
      route: this.route,
      port: this.port,
      lifetime: lifetime(this.openedAt === undefined ? undefined : Date.now() - this.openedAt),
    };
    return { ...this.last };
  }
  get lastClose(): ConnectionCloseDiagnostic | undefined {
    return this.last && { ...this.last };
  }
}

export const maximumTimerLateness = (...values: TimerLatenessBucket[]): TimerLatenessBucket => {
  const buckets: TimerLatenessBucket[] = ['unknown', '<1s', '1-5s', '5-20s', '20-60s', '>=60s'];
  return buckets[Math.max(...values.map((value) => buckets.indexOf(value)))];
};
