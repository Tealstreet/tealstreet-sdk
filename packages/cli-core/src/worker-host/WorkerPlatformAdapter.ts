import type { CLIPlatformAdapter, ExchangeLike, ExchangeAccountConfig } from '../types';
import { setTableRenderMode } from '../table/Table';

/**
 * Platform adapter for webworker context.
 * Routes output through exchange.emitter for the web app to display.
 * Account management is handled by the web app main thread, so those methods are stubs.
 */
export class WorkerPlatformAdapter implements CLIPlatformAdapter {
  private exchange: ExchangeLike;
  /** Set when the browser asked with a reqId: every line it prints carries
   *  it, so one caller (the command bar) can collect exactly its own output. */
  private reqId?: string;

  constructor(exchange: ExchangeLike, reqId?: string) {
    this.reqId = reqId;
    this.exchange = exchange;
    // Set table render mode to 'web' when adapter is instantiated (output goes to web UI)
    // This is set in constructor rather than module level to avoid polluting CLI mode
    // when this file is imported transitively
    setTableRenderMode('web');
  }

  // Account management - not used in webworker (handled by main thread)
  listAccounts(): ExchangeAccountConfig[] {
    return [];
  }

  getAccount(_name?: string): ExchangeAccountConfig | undefined {
    return undefined;
  }

  addAccount(_account: ExchangeAccountConfig): void {
    // No-op in webworker
  }

  updateAccount(_name: string, _updates: Partial<ExchangeAccountConfig>): boolean {
    return false;
  }

  removeAccount(_name: string): boolean {
    return false;
  }

  setDefaultAccount(_name: string): boolean {
    return false;
  }

  getDefaultSymbol(): string | undefined {
    return undefined;
  }

  setDefaultSymbol(_symbol: string): void {
    // No-op in webworker
  }

  getActiveAccounts(): ExchangeAccountConfig[] {
    return [];
  }

  // Exchange management - not used in webworker (exchange already exists)
  async createExchange(_account: ExchangeAccountConfig): Promise<ExchangeLike> {
    return this.exchange;
  }

  async startExchange(_exchange: ExchangeLike): Promise<void> {
    // No-op - exchange already running
  }

  async stopExchange(_exchange: ExchangeLike): Promise<void> {
    // No-op - lifecycle managed by main thread
  }

  // Output methods - emit through exchange.emitter
  private log(message: string): void {
    this.exchange.emitter.emit('exec-log', this.reqId === undefined ? { message } : { message, reqId: this.reqId });
  }

  print(lines: string[]): void {
    this.log(lines.join('\n'));
  }

  printError(message: string): void {
    this.log(`Error: ${message}`);
  }

  printSuccess(message: string): void {
    this.log(message);
  }

  printWarning(message: string): void {
    this.log(`Warning: ${message}`);
  }

  printInfo(message: string): void {
    this.log(message);
  }
}
