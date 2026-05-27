import type { Command, CommandCategory, CommandContext, CommandResult } from './Command';
import { CommandType, CommandAvailability } from './Command';

/**
 * Abstract base class for commands - provides common functionality
 */
export abstract class BaseCommand implements Command {
  abstract readonly name: string;
  abstract readonly description: string;
  abstract readonly category: CommandCategory;

  /** Override in subclass if command has aliases */
  readonly aliases: string[] = [];

  /** Override in subclass to provide examples */
  readonly examples: string[] = [];

  /** Override to false for commands that work without connection */
  readonly requiresConnection: boolean = true;

  /** Default: instant command (runs and returns immediately) */
  readonly type: CommandType = CommandType.INSTANT;

  /** Default: available in both CLI args and REPL modes */
  readonly availability: CommandAvailability = CommandAvailability.BOTH;

  /**
   * Check if this command matches the given name
   */
  matches(commandName: string): boolean {
    const lower = commandName.toLowerCase();
    return this.name === lower || this.aliases.includes(lower);
  }

  /**
   * Execute the command - must be implemented by subclass
   */
  abstract execute(ctx: CommandContext): Promise<CommandResult>;

  /**
   * Helper: Create a success result
   */
  protected success(message?: string): CommandResult {
    return { success: true, message };
  }

  /**
   * Helper: Create a success result that carries a structured payload.
   * Used by order-placing commands to surface `orderIds` to the runner's
   * capture-to-var path (`o1 = buy …`). The data field is opt-in — most
   * commands have no IDs to surface and use plain `success()`.
   */
  protected successWithData(data: CommandResult['data'], message?: string): CommandResult {
    return { success: true, message, data };
  }

  /**
   * Helper: Create an error result
   */
  protected error(error: string): CommandResult {
    return { success: false, error };
  }

  /**
   * Helper: Print output via adapter
   */
  protected print(ctx: CommandContext, lines: string[]): void {
    ctx.adapter.print(lines);
  }

  /**
   * Helper: Print info message
   */
  protected printInfo(ctx: CommandContext, message: string): void {
    ctx.adapter.printInfo(message);
  }

  /**
   * Helper: Print error message
   */
  protected printError(ctx: CommandContext, message: string): void {
    ctx.adapter.printError(message);
  }

  /**
   * Helper: Print success message
   */
  protected printSuccess(ctx: CommandContext, message: string): void {
    ctx.adapter.printSuccess(message);
  }

  /**
   * Helper: Print warning message
   */
  protected printWarning(ctx: CommandContext, message: string): void {
    ctx.adapter.printWarning(message);
  }

  /**
   * Helper: Get positional argument (0 = command name, 1 = first arg, etc.)
   */
  protected getArg(ctx: CommandContext, index: number): string | undefined {
    return ctx.args._[index];
  }

  /**
   * Helper: Get all positional arguments after command name
   */
  protected getArgs(ctx: CommandContext): string[] {
    return ctx.args._.slice(1);
  }

  /**
   * Helper: Check if a flag is present
   */
  protected hasFlag(ctx: CommandContext, ...names: string[]): boolean {
    return names.some((name) => ctx.args[name] === true);
  }

  /**
   * Helper: Get option value
   */
  protected getOption(ctx: CommandContext, ...names: string[]): string | undefined {
    for (const name of names) {
      const value = ctx.args[name];
      if (typeof value === 'string') {
        return value;
      }
    }
    return undefined;
  }

  /**
   * Dry-run gate. If `simulation on` is set in the preprocess store,
   * log the (already-substituted) command and bail before any
   * exchange interaction. Caller convention:
   *
   *   async execute(ctx) {
   *     if (this.simulationGate(ctx)) return this.success();
   *     // …live path…
   *   }
   *
   * Returns `true` when the caller should bail. Read commands
   * (positions / orders / margin / etc.) deliberately do NOT call
   * this — sim should still let the user inspect state.
   */
  protected simulationGate(ctx: CommandContext, label?: string): boolean {
    if (!ctx.preprocessStore?.getSimulation()) return false;
    const text = label ?? ctx.raw ?? this.name;
    this.printInfo(ctx, `[sim] ${text}`);
    return true;
  }

  /**
   * Health gate for exposure-INCREASING writes (TSBUG-106). When the
   * account's private data stream was up and has gone stale, the
   * position/order snapshot can't be trusted, so opening or increasing
   * exposure blindly is dangerous — return an error string so the
   * caller can bail. Risk-REDUCING ops (close / cancel / nuke /
   * reduce-only) must NOT call this: blocking them during a degraded
   * connection would trap the user in a position they can't exit.
   *
   * Fails OPEN (returns undefined) when we can't tell — no private
   * socket initialized yet, or the exchange doesn't report
   * `socketHealth` — so it never blocks normal trading on a false
   * positive. The signal is computed in BaseExchange for every
   * exchange: `privateInitialized && !privateHealthy` means every
   * initialized private socket is currently unhealthy.
   *
   *   if (!reduceOnly) { const b = this.healthGate(ctx); if (b) return this.error(b); }
   */
  protected healthGate(ctx: CommandContext): string | undefined {
    const h = ctx.exchange?.store?.socketHealth;
    if (h && h.privateInitialized && !h.privateHealthy) {
      return 'Account connection is degraded (private data stream is stale) — refusing to open or increase exposure. Wait for reconnect (run `health`); close / cancel / reduce-only orders stay allowed.';
    }
    return undefined;
  }
}
