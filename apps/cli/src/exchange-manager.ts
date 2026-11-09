import { getStorageOwner, getStorageEpoch, assertStorageSession, onStorageOwnerChange } from './storage-context';
import type { ExchangeLike, CLIPlatformAdapter, ExchangeAccountConfig } from '@tealstreet/cli-core';
import { ExchangeHealthMonitor, type HealthState } from '../../../packages/safe-cex/src/health/ExchangeHealthMonitor';

/**
 * Running exchange instance with metadata
 */
export interface RunningExchange {
  exchange: ExchangeLike;
  accountName: string;
  accountConfig: ExchangeAccountConfig;
}

/**
 * Manages multiple running exchange connections.
 * Tracks which accounts are connected and provides access to all running exchanges.
 */
export class ExchangeManager {
  private exchanges: Map<string, RunningExchange> = new Map();
  private healthMonitors: Map<string, ExchangeHealthMonitor> = new Map();
  private adapter: Pick<CLIPlatformAdapter, 'createExchange' | 'startExchange' | 'stopExchange' | 'updateAccount'>;

  constructor(
    adapter: Pick<CLIPlatformAdapter, 'createExchange' | 'startExchange' | 'stopExchange' | 'updateAccount'>,
  ) {
    this.adapter = adapter;
    onStorageOwnerChange(() => {
      void this.stopAll().catch(() => {});
    });
  }

  private key(name: string): string {
    return JSON.stringify([getStorageOwner(), name]);
  }

  /**
   * Check if an account is currently connected
   */
  isConnected(accountName: string): boolean {
    return this.exchanges.has(this.key(accountName));
  }

  /**
   * Get a running exchange by account name
   */
  get(accountName: string): RunningExchange | undefined {
    return this.exchanges.get(this.key(accountName));
  }

  /**
   * Get the exchange instance for an account
   */
  getExchange(accountName: string): ExchangeLike | undefined {
    return this.exchanges.get(this.key(accountName))?.exchange;
  }

  /**
   * Get all running exchanges
   */
  getAll(): RunningExchange[] {
    assertStorageSession();
    return Array.from(this.exchanges.values());
  }

  /**
   * Get all account names that are connected
   */
  getConnectedAccountNames(): string[] {
    return this.getAll().map((running) => running.accountName);
  }

  /**
   * Get all connected accounts with their exchange instances
   * Returns array of { name, exchange } objects
   */
  getConnectedAccounts(): Array<{ name: string; exchange: ExchangeLike }> {
    return this.getAll().map((running) => ({
      name: running.accountName,
      exchange: running.exchange,
    }));
  }

  /**
   * Get count of connected exchanges
   */
  get size(): number {
    assertStorageSession();
    return this.exchanges.size;
  }

  /**
   * Get health monitor for an account
   */
  getHealthMonitor(accountName: string): ExchangeHealthMonitor | undefined {
    return this.healthMonitors.get(this.key(accountName));
  }

  /**
   * Get health states for all connected accounts
   */
  getAllHealthStates(): Map<string, HealthState> {
    assertStorageSession();
    const states = new Map<string, HealthState>();
    for (const [name, monitor] of this.healthMonitors) {
      states.set(JSON.parse(name)[1], monitor.getState());
    }
    return states;
  }

  /**
   * Enable an account - create exchange, start it, and track it
   * Returns the exchange instance
   */
  async enable(account: ExchangeAccountConfig): Promise<ExchangeLike> {
    // Check if already connected
    const epoch = getStorageEpoch();
    const existing = this.get(account.name);
    if (existing) return existing.exchange;

    // Create and start the exchange
    const exchange = await this.adapter.createExchange(account);
    try {
      assertStorageSession(epoch);
      await this.adapter.startExchange(exchange);
      assertStorageSession(epoch);
    } catch (error) {
      await this.adapter.stopExchange(exchange);
      throw error;
    }

    // Track it
    this.exchanges.set(this.key(account.name), {
      exchange,
      accountName: account.name,
      accountConfig: account,
    });

    // Create health monitor for this exchange
    this.createHealthMonitor(account.name, account.exchange);

    // Update the active state in config
    this.adapter.updateAccount(account.name, { active: true });

    return exchange;
  }

  /**
   * Disable an account - stop the exchange and remove from tracking
   */
  async disable(accountName: string): Promise<void> {
    const running = this.exchanges.get(this.key(accountName));
    if (!running) {
      return;
    }

    const epoch = getStorageEpoch();
    this.exchanges.delete(this.key(accountName));
    this.disposeHealthMonitor(accountName);
    await this.adapter.stopExchange(running.exchange);
    assertStorageSession(epoch);

    // Update the active state in config
    this.adapter.updateAccount(accountName, { active: false });
  }

  /**
   * Stop all exchanges
   */
  async stopAll(): Promise<void> {
    assertStorageSession();
    // Detach synchronously before awaiting disposal. A newly logged-in user's
    // instances must never be cleared by completion of the previous stop.
    const running = Array.from(this.exchanges.values());
    this.exchanges.clear();
    for (const monitor of this.healthMonitors.values()) monitor.dispose();
    this.healthMonitors.clear();
    await Promise.all(running.map(({ exchange }) => this.adapter.stopExchange(exchange)));
  }

  /**
   * Register an already-running exchange (used when switching primary account)
   */
  register(accountName: string, exchange: ExchangeLike, accountConfig: ExchangeAccountConfig): void {
    this.exchanges.set(this.key(accountName), {
      exchange,
      accountName,
      accountConfig,
    });

    // Create health monitor if not already present
    if (!this.healthMonitors.has(this.key(accountName))) {
      this.createHealthMonitor(accountName, accountConfig.exchange);
    }

    // Update the active state in config
    this.adapter.updateAccount(accountName, { active: true });
  }

  /**
   * Unregister an exchange without stopping it (used when switching primary account)
   */
  unregister(accountName: string): ExchangeLike | undefined {
    const running = this.exchanges.get(this.key(accountName));
    if (running) {
      this.exchanges.delete(this.key(accountName));
      this.disposeHealthMonitor(accountName);
      return running.exchange;
    }
    return undefined;
  }

  /**
   * Create a health monitor for an account
   */
  private createHealthMonitor(accountName: string, exchangeName: string): void {
    this.disposeHealthMonitor(accountName);
    const monitor = new ExchangeHealthMonitor(exchangeName);
    this.healthMonitors.set(this.key(accountName), monitor);
  }

  /**
   * Dispose a health monitor for an account
   */
  private disposeHealthMonitor(accountName: string): void {
    const existing = this.healthMonitors.get(this.key(accountName));
    if (existing) {
      existing.dispose();
      this.healthMonitors.delete(this.key(accountName));
    }
  }
}
