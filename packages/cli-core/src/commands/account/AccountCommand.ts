import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * Account command - switch to a specific account
 */
export class AccountCommand extends BaseCommand {
  readonly name = 'account';
  readonly description = 'Switch to a specific account';
  readonly category = CommandCategory.Account;
  readonly aliases = ['acc', 'use'];
  readonly examples = ['account mybybit', 'account 1', 'account -# 2', 'use binance-main', 'account enable acc1', 'account enable --all', 'account disable --all'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const firstArg = this.getArg(ctx, 1);

    // Check if this is an enable/disable command (unless there's an account with that name)
    if (firstArg === 'enable' || firstArg === 'on') {
      // Only treat as enable command if there's no account named "enable"/"on"
      const accountWithName = ctx.adapter.getAccount(firstArg);
      if (!accountWithName) {
        return this.handleEnableDisable(ctx, 'enable');
      }
    }

    if (firstArg === 'disable' || firstArg === 'off') {
      // Only treat as disable command if there's no account named "disable"/"off"
      const accountWithName = ctx.adapter.getAccount(firstArg);
      if (!accountWithName) {
        return this.handleEnableDisable(ctx, 'disable');
      }
    }

    // Check for -# flag to switch by number
    const numberArg = this.getOption(ctx, '#');
    let accountName = firstArg;

    if (numberArg) {
      const accountIndex = parseInt(numberArg, 10) - 1; // Convert to 0-based index
      const accounts = ctx.adapter.listAccounts();

      if (isNaN(accountIndex) || accountIndex < 0 || accountIndex >= accounts.length) {
        this.printError(ctx, `Invalid account number: ${numberArg}`);
        this.printInfo(ctx, `Valid range: 1-${accounts.length}`);
        return this.success();
      }

      accountName = accounts[accountIndex].name;
    } else if (firstArg && /^\d+$/.test(firstArg)) {
      // Check if firstArg is a pure number (e.g., "1", "2")
      // Treat as account number if no account exists with that literal name
      const accountWithName = ctx.adapter.getAccount(firstArg);
      if (!accountWithName) {
        const accountIndex = parseInt(firstArg, 10) - 1;
        const accounts = ctx.adapter.listAccounts();

        if (accountIndex >= 0 && accountIndex < accounts.length) {
          accountName = accounts[accountIndex].name;
        }
        // If invalid number, let it fall through to "account not found" error
      }
    }

    if (!accountName) {
      // Show current account
      if (ctx.accountName) {
        this.printInfo(ctx, `Current account: ${ctx.accountName}`);
      } else {
        this.printWarning(ctx, 'Not connected to any account');
      }
      this.printInfo(ctx, 'Usage: account <name> or account <number>');
      this.printInfo(ctx, 'Use "accounts" to list available accounts');
      return this.success();
    }

    const account = ctx.adapter.getAccount(accountName);
    if (!account) {
      this.printError(ctx, `Account "${accountName}" not found`);
      this.printInfo(ctx, 'Use "accounts" to list available accounts');
      return this.success();
    }

    // Check if this account is already connected (enabled)
    const existingExchange = ctx.adapter.getExchange?.(accountName);
    if (existingExchange) {
      // Account is already connected - just switch to it without reloading
      this.printSuccess(ctx, `Switched to ${account.exchange}`);

      return {
        success: true,
        updateSession: {
          exchange: existingExchange,
          accountName: account.name,
        },
      };
    }

    // Account not connected - need to create and start exchange
    this.printInfo(ctx, `Connecting to ${account.exchange}...`);

    // Stop current exchange if connected AND it's not tracked by exchangeManager
    // (i.e., it was a standalone primary connection, not an enabled account)
    // Note: We don't stop enabled accounts - they keep running in background
    if (ctx.exchange && !ctx.adapter.isAccountConnected?.(ctx.accountName ?? '')) {
      await ctx.adapter.stopExchange(ctx.exchange);
    }

    try {
      const newExchange = await ctx.adapter.createExchange(account);
      await ctx.adapter.startExchange(newExchange);
      this.printSuccess(ctx, `Connected to ${account.exchange}`);

      return {
        success: true,
        updateSession: {
          exchange: newExchange,
          accountName: account.name,
        },
      };
    } catch (error) {
      this.printError(ctx, `Failed to connect: ${(error as Error).message}`);
      return this.success();
    }
  }

  /**
   * Handle enable/disable for multiple accounts
   * Supports: account enable acc1 acc2 acc3
   *           account enable -# 1 2 3
   *           account enable --all
   *           account disable acc1 acc2
   *           account disable --all
   */
  private handleEnableDisable(ctx: CommandContext, action: 'enable' | 'disable'): CommandResult {
    // Check for --all flag
    if (this.hasFlag(ctx, 'all')) {
      const accounts = ctx.adapter.listAccounts();
      const accountNames = accounts.map((acc) => acc.name);

      if (accountNames.length === 0) {
        this.printInfo(ctx, `No accounts to ${action}`);
        return this.success();
      }

      this.printInfo(ctx, `${action === 'enable' ? 'Enabling' : 'Disabling'} ${accountNames.length} account(s)...`);

      return {
        success: true,
        accountAction: {
          action,
          accountNames: accountNames,
        },
      };
    }

    // Get all arguments after enable/disable
    const allArgs = ctx.args._;
    const accountNames: string[] = [];

    // Check for -# flag for numbered accounts
    const numberArgs = ctx.args['#'];

    if (numberArgs) {
      // Handle numbered accounts: -# 1 2 3 or -# "1 2 3"
      const numbers = Array.isArray(numberArgs) ? numberArgs : [numberArgs];
      const accounts = ctx.adapter.listAccounts();

      for (const num of numbers) {
        // Handle space-separated numbers in a single string
        const parts = String(num).split(/\s+/);
        for (const part of parts) {
          const accountIndex = parseInt(part, 10) - 1;
          if (!isNaN(accountIndex) && accountIndex >= 0 && accountIndex < accounts.length) {
            accountNames.push(accounts[accountIndex].name);
          } else {
            this.printWarning(ctx, `Invalid account number: ${part}`);
          }
        }
      }
    }

    // Also get positional arguments after "enable"/"disable" (index 2, 3, 4, ...)
    for (let i = 2; i < allArgs.length; i++) {
      const name = allArgs[i];
      if (name && !accountNames.includes(name)) {
        accountNames.push(name);
      }
    }

    if (accountNames.length === 0) {
      this.printWarning(ctx, `Usage: account ${action} <name1> [name2] ... or account ${action} --all`);
      return this.success();
    }

    // Validate all accounts exist
    const validAccounts: string[] = [];
    for (const name of accountNames) {
      const account = ctx.adapter.getAccount(name);
      if (!account) {
        this.printError(ctx, `Account "${name}" not found`);
      } else {
        validAccounts.push(name);
      }
    }

    if (validAccounts.length === 0) {
      return this.success();
    }

    // Return with action to enable/disable the accounts
    return {
      success: true,
      accountAction: {
        action,
        accountNames: validAccounts,
      },
    };
  }
}
