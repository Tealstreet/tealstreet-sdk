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
  private adapter: CLIPlatformAdapter;

  constructor(adapter: CLIPlatformAdapter) {
    this.adapter = adapter;
  }

  /**
   * Check if an account is currently connected
   */
  isConnected(accountName: string): boolean {
    return this.exchanges.has(accountName);
  }

  /**
   * Get a running exchange by account name
   */
  get(accountName: string): RunningExchange | undefined {
    return this.exchanges.get(accountName);
  }

  /**
   * Get the exchange instance for an account
   */
  getExchange(accountName: string): ExchangeLike | undefined {
    return this.exchanges.get(accountName)?.exchange;
  }

  /**
   * Get all running exchanges
   */
  getAll(): RunningExchange[] {
    return Array.from(this.exchanges.values());
  }

  /**
   * Get all account names that are connected
   */
  getConnectedAccountNames(): string[] {
    return Array.from(this.exchanges.keys());
  }

  /**
   * Get all connected accounts with their exchange instances
   * Returns array of { name, exchange } objects
   */
  getConnectedAccounts(): Array<{ name: string; exchange: ExchangeLike }> {
    return Array.from(this.exchanges.entries()).map(([name, running]) => ({
      name,
      exchange: running.exchange,
    }));
  }

  /**
   * Get count of connected exchanges
   */
  get size(): number {
    return this.exchanges.size;
  }

  /**
   * Get health monitor for an account
   */
  getHealthMonitor(accountName: string): ExchangeHealthMonitor | undefined {
    return this.healthMonitors.get(accountName);
  }

  /**
   * Get health states for all connected accounts
   */
  getAllHealthStates(): Map<string, HealthState> {
    const states = new Map<string, HealthState>();
    for (const [name, monitor] of this.healthMonitors) {
      states.set(name, monitor.getState());
    }
    return states;
  }

  /**
   * Enable an account - create exchange, start it, and track it
   * Returns the exchange instance
   */
  /**
   * @param opts.persistActive When false, do NOT write `active: true` back to
   *   config.json. Required for any SECOND process (e.g. the MCP server)
   *   connecting alongside a running REPL: `updateAccount` rewrites the whole
   *   config file with no lock, so persisting from a second process races the
   *   REPL's own writes — last-writer-wins can clobber edits and resurrect
   *   deleted-account tombstones — and it silently changes which accounts the
   *   REPL auto-connects next launch.
   */
  async enable(account: ExchangeAccountConfig, opts: { persistActive?: boolean } = {}): Promise<ExchangeLike> {
    // Check if already connected
    if (this.exchanges.has(account.name)) {
      return this.exchanges.get(account.name)!.exchange;
    }

    // Create and start the exchange
    const exchange = await this.adapter.createExchange(account);
    await this.adapter.startExchange(exchange);

    // Track it
    this.exchanges.set(account.name, {
      exchange,
      accountName: account.name,
      accountConfig: account,
    });

    // Create health monitor for this exchange
    this.createHealthMonitor(account.name, account.exchange);

    // Update the active state in config (skipped for read-only side processes)
    if (opts.persistActive !== false) {
      this.adapter.updateAccount(account.name, { active: true });
    }

    return exchange;
  }

  /**
   * Disable an account - stop the exchange and remove from tracking
   */
  async disable(accountName: string): Promise<void> {
    const running = this.exchanges.get(accountName);
    if (!running) {
      return;
    }

    // Stop the exchange
    await this.adapter.stopExchange(running.exchange);

    // Dispose health monitor
    this.disposeHealthMonitor(accountName);

    // Remove from tracking
    this.exchanges.delete(accountName);

    // Update the active state in config
    this.adapter.updateAccount(accountName, { active: false });
  }

  /**
   * Stop all exchanges
   */
  async stopAll(): Promise<void> {
    const stopPromises = Array.from(this.exchanges.values()).map(async (running) => {
      await this.adapter.stopExchange(running.exchange);
    });
    await Promise.all(stopPromises);
    this.exchanges.clear();

    // Dispose all health monitors
    for (const [, monitor] of this.healthMonitors) {
      monitor.dispose();
    }
    this.healthMonitors.clear();
  }

  /**
   * Register an already-running exchange (used when switching primary account)
   */
  register(accountName: string, exchange: ExchangeLike, accountConfig: ExchangeAccountConfig): void {
    this.exchanges.set(accountName, {
      exchange,
      accountName,
      accountConfig,
    });

    // Create health monitor if not already present
    if (!this.healthMonitors.has(accountName)) {
      this.createHealthMonitor(accountName, accountConfig.exchange);
    }

    // Update the active state in config
    this.adapter.updateAccount(accountName, { active: true });
  }

  /**
   * Unregister an exchange without stopping it (used when switching primary account)
   */
  unregister(accountName: string): ExchangeLike | undefined {
    const running = this.exchanges.get(accountName);
    if (running) {
      this.exchanges.delete(accountName);
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
    this.healthMonitors.set(accountName, monitor);
  }

  /**
   * Dispose a health monitor for an account
   */
  private disposeHealthMonitor(accountName: string): void {
    const existing = this.healthMonitors.get(accountName);
    if (existing) {
      existing.dispose();
      this.healthMonitors.delete(accountName);
    }
  }
}
