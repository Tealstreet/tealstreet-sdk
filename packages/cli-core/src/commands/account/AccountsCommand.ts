import {
  buildExchangeAccountConfig,
  ExchangeAccountConfigError,
} from '@tealstreet/safe-cex/src/accounts/buildExchangeAccountConfig';

import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { getExchangeInfo } from '../../exchanges';
import type { AccountConfig, ExchangeName } from '../../types';
import { getConnectionStatus, type ConnectionHealth } from '../../utils/ConnectionMonitor';
import { getCliBrand } from '../../brand';

/**
 * Web account export format - the JSON structure from web's account export
 */
interface WebAccountExport {
  exchange: string;
  name: string;
  apiKey?: string;
  apiSecret?: string;
  secret?: string;
  passphrase?: string;
  privateKey?: string;
  walletAddress?: string;
  applicationId?: string;
  apiKeyIndex?: string;
  vault?: string;
  omniKeySeed?: string;
  paradexAddress?: string;
  subkeyAddress?: string;
  subaccount?: string;
  grvtSignerAddress?: string;
  grvtTradingAccountId?: string;
  testnet?: boolean;
  // Tunnel/proxy settings
  tealstreetProxy?: boolean;
  tealstreetTunnel?: boolean;
  proxyRegion?: string;
  // Fields we ignore but may be present
  color?: string;
  active?: boolean;
  lastSymbol?: string;
  user?: string;
}

/**
 * Accounts command - list, add, remove, import accounts
 */
export class AccountsCommand extends BaseCommand {
  readonly name = 'accounts';
  readonly description = 'Manage accounts (list, add, remove, import)';
  readonly category = CommandCategory.Account;
  readonly aliases = ['config'];
  readonly examples = [
    'accounts',
    'accounts enabled',
    'accounts add -n mybybit -e bybit -k KEY -s SECRET',
    'accounts add -n mybybit -e bybit -k KEY -s SECRET --proxy',
    'accounts import {"exchange":"bybit","name":"my-acc",...}',
    'accounts remove mybybit',
    'accounts remove --all',
    'accounts enable mybybit',
    'accounts enable --all',
    'accounts disable mybybit',
    'accounts disable --all',
    'accounts help',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const subCommand = this.getArg(ctx, 1);

    if (!subCommand || subCommand === 'list') {
      return this.listAccounts(ctx);
    }

    if (subCommand === 'add') {
      return this.addAccount(ctx);
    }

    if (subCommand === 'import') {
      return this.importAccount(ctx);
    }

    if (subCommand === 'remove' || subCommand === 'rm') {
      return this.removeAccount(ctx);
    }

    if (subCommand === 'enable' || subCommand === 'on') {
      return this.enableAccount(ctx);
    }

    if (subCommand === 'disable' || subCommand === 'off') {
      return this.disableAccount(ctx);
    }

    if (subCommand === 'enabled' || subCommand === 'streaming' || subCommand === 'active') {
      return this.listEnabledAccounts(ctx);
    }

    if (subCommand === 'help') {
      return this.showHelp(ctx);
    }

    // Check if it looks like JSON (starts with {) - treat as import
    if (subCommand.startsWith('{')) {
      // User pasted JSON directly after 'accounts' - use raw input to preserve JSON
      const match = ctx.raw.match(/^\s*accounts\s+(.+)$/i);
      const jsonStr = match ? match[1] : '';
      return this.importAccountFromJson(ctx, jsonStr);
    }

    // Unknown subcommand - show help
    this.printWarning(ctx, `Unknown subcommand: ${subCommand}`);
    return this.showHelp(ctx);
  }

  private listAccounts(ctx: CommandContext): CommandResult {
    const accounts = ctx.adapter.listAccounts();

    // ANSI color codes
    const GREEN = '\x1b[32m';
    const YELLOW = '\x1b[33m';
    const RED = '\x1b[31m';
    const GRAY = '\x1b[90m';
    const CYAN = '\x1b[36m';
    const RESET = '\x1b[0m';

    if (accounts.length === 0) {
      this.printWarning(ctx, 'No accounts configured.');
      this.printInfo(ctx, 'Use "accounts add" to add an account.');
    } else {
      this.print(ctx, ['', 'Configured Accounts:', '']);
      accounts.forEach((acc, index) => {
        const isCurrent = acc.name === ctx.accountName;
        const isConnected = ctx.adapter.isAccountConnected?.(acc.name) ?? false;

        // Determine connection health color
        let dot: string;
        if (!isConnected) {
          // Not connected - gray dot
          dot = `${GRAY}●${RESET}`;
        } else {
          // Connected - check health status
          const exchange = ctx.adapter.getExchange?.(acc.name);
          if (exchange) {
            const status = getConnectionStatus(exchange);
            dot = this.getConnectionDot(status.overall, GREEN, YELLOW, RED);
          } else {
            // Fallback: connected but no exchange access - assume healthy
            dot = `${GREEN}●${RESET}`;
          }
        }

        const num = String(index + 1).padStart(2, ' ');
        const suffix = isCurrent ? ' (primary)' : '';
        // Color the entire row cyan for primary account
        if (isCurrent) {
          this.print(ctx, [`${dot} ${CYAN}${num}. ${acc.name} - ${acc.exchange}${suffix}${RESET}`]);
        } else {
          this.print(ctx, [`${dot} ${num}. ${acc.name} - ${acc.exchange}${suffix}`]);
        }
      });
      this.print(ctx, ['', 'Switch by number: account 1', '']);
    }

    return this.success();
  }

  /**
   * Get colored dot based on connection health
   */
  private getConnectionDot(health: ConnectionHealth, GREEN: string, YELLOW: string, RED: string): string {
    const RESET = '\x1b[0m';
    switch (health) {
      case 'healthy':
        return `${GREEN}●${RESET}`;
      case 'warning':
      case 'initializing':
        return `${YELLOW}●${RESET}`;
      case 'error':
      case 'disconnected':
        return `${RED}●${RESET}`;
      default:
        return `${GREEN}●${RESET}`;
    }
  }

  private addAccount(ctx: CommandContext): CommandResult {
    // Parse options from args
    const name = this.getOption(ctx, 'n', 'name');
    const exchange = this.getOption(ctx, 'e', 'exchange');
    const apiKey = this.getOption(ctx, 'k', 'api-key');
    const apiSecret = this.getOption(ctx, 's', 'api-secret');
    const passphrase = this.getOption(ctx, 'p', 'passphrase');
    const privateKey = this.getOption(ctx, 'private-key');
    const walletAddress = this.getOption(ctx, 'wallet-address');
    const applicationId = this.getOption(ctx, 'application-id');
    const apiKeyIndex = this.getOption(ctx, 'api-key-index');
    const vault = this.getOption(ctx, 'vault');
    const omniKeySeed = this.getOption(ctx, 'omni-key-seed');
    const paradexAddress = this.getOption(ctx, 'paradex-address');
    const subkeyAddress = this.getOption(ctx, 'subkey-address');
    const subaccount = this.getOption(ctx, 'subaccount');
    const grvtSignerAddress = this.getOption(ctx, 'grvt-signer-address');
    const grvtTradingAccountId = this.getOption(ctx, 'grvt-trading-account-id');
    const proxyRegion = this.getOption(ctx, 'proxy-region');
    const testnet = this.hasFlag(ctx, 't', 'testnet');
    // Both spellings on every brand; help shows the brand's own (see brand.ts).
    const tealstreetProxy = this.hasFlag(ctx, 'proxy', 'tealstreet-proxy', 'use-tealstreet-ips');
    const tealstreetTunnel = this.hasFlag(ctx, 'tunnel', 'tealstreet-tunnel');
    const tealstreetWsRoute = this.hasFlag(ctx, 'ws-route', 'tealstreet-ws-route');
    const enableSpotMarkets = this.hasFlag(ctx, 'enable-spot-markets');

    // Validate required fields
    if (!name || !exchange) {
      this.printWarning(
        ctx,
        'Usage: accounts add -n <name> -e <exchange> -k <api-key> -s <api-secret> [-p <passphrase>] [-t]',
      );
      this.printInfo(
        ctx,
        '       accounts add -n <name> -e <exchange> --private-key <key> --wallet-address <addr> [-t]',
      );
      this.printInfo(ctx, 'Use "accounts help" to see all options.');
      return this.success();
    }

    // Validate exchange
    const exchangeInfo = getExchangeInfo(exchange);
    if (!exchangeInfo) {
      this.printError(ctx, `Unknown exchange: ${exchange}`);
      this.printInfo(ctx, 'Use "exchanges" to see supported exchanges');
      return this.success();
    }

    let newAccount: AccountConfig;
    try {
      newAccount = buildExchangeAccountConfig({
        name,
        exchange: exchange.toLowerCase() as ExchangeName,
        apiKey,
        apiSecret,
        passphrase,
        privateKey,
        walletAddress,
        applicationId,
        apiKeyIndex,
        vault,
        omniKeySeed,
        paradexAddress,
        subkeyAddress,
        subaccount,
        grvtSignerAddress,
        grvtTradingAccountId,
        testnet,
        tealstreetProxy,
        tealstreetTunnel,
        tealstreetWsRoute,
        proxyRegion,
        enableSpotMarkets,
        active: true, // Always set active on add so it will be enabled
      });
    } catch (error) {
      if (error instanceof ExchangeAccountConfigError) {
        this.printError(ctx, error.message);
      } else {
        this.printError(ctx, error instanceof Error ? error.message : String(error));
      }
      return this.success();
    }

    ctx.adapter.addAccount(newAccount);
    this.printSuccess(ctx, `Account "${newAccount.name}" added successfully`);

    // Enable the account (start streaming)
    // If no primary, hint user to switch
    if (!ctx.accountName) {
      this.printInfo(ctx, `Use "account ${newAccount.name}" to set as primary account`);
    }

    return {
      success: true,
      accountAction: {
        action: 'enable',
        accountNames: [newAccount.name],
      },
    };
  }

  private importAccount(ctx: CommandContext): CommandResult {
    // Get everything after 'accounts import' from raw input to preserve JSON
    const match = ctx.raw.match(/^\s*accounts\s+import\s+(.+)$/i);
    const jsonStr = match ? match[1] : '';
    return this.importAccountFromJson(ctx, jsonStr);
  }

  private importAccountFromJson(ctx: CommandContext, jsonStr: string): CommandResult {
    if (!jsonStr || !jsonStr.trim()) {
      this.printWarning(ctx, 'Usage: accounts import <json>');
      this.printInfo(ctx, `Paste the JSON export from ${getCliBrand().displayName} web`);
      this.printInfo(
        ctx,
        'Example: accounts import {"exchange":"bybit","name":"my-acc","apiKey":"...","apiSecret":"..."}',
      );
      return this.success();
    }

    let parsed: WebAccountExport;
    try {
      parsed = JSON.parse(jsonStr.trim());
    } catch {
      this.printError(ctx, 'Invalid JSON format');
      this.printInfo(ctx, 'Make sure you paste the complete JSON string from the web export');
      return this.success();
    }

    // Validate required fields
    if (!parsed.exchange) {
      this.printError(ctx, 'Missing "exchange" field in JSON');
      return this.success();
    }

    if (!parsed.name) {
      this.printError(ctx, 'Missing "name" field in JSON');
      return this.success();
    }

    // Validate exchange
    const exchangeInfo = getExchangeInfo(parsed.exchange);
    if (!exchangeInfo) {
      this.printError(ctx, `Unknown exchange: ${parsed.exchange}`);
      this.printInfo(ctx, 'Use "exchanges" to see supported exchanges');
      return this.success();
    }

    // Check for existing account with same name
    const existingAccount = ctx.adapter.getAccount(parsed.name);
    if (existingAccount) {
      this.printError(ctx, `Account "${parsed.name}" already exists`);
      this.printInfo(ctx, 'Use a different name or remove the existing account first');
      return this.success();
    }

    // Build account config from parsed JSON
    let newAccount: AccountConfig;
    try {
      newAccount = buildExchangeAccountConfig({
        ...parsed,
        exchange: parsed.exchange.toLowerCase() as ExchangeName,
        active: true, // Always set active on import so it will be enabled
      });
    } catch (error) {
      this.printError(ctx, error instanceof Error ? error.message : String(error));
      return this.success();
    }

    ctx.adapter.addAccount(newAccount);
    this.printSuccess(ctx, `Account "${newAccount.name}" imported successfully`);
    this.printInfo(ctx, `Exchange: ${exchangeInfo.displayName}${newAccount.testnet ? ' (testnet)' : ''}`);
    if (newAccount.tealstreetTunnel) {
      this.printInfo(ctx, `Tunnel: enabled${newAccount.proxyRegion ? ` (region: ${newAccount.proxyRegion})` : ''}`);
    }

    // Enable the account (start streaming)
    // If no primary, hint user to switch
    if (!ctx.accountName) {
      this.printInfo(ctx, `Use "account ${newAccount.name}" to set as primary account`);
    }

    return {
      success: true,
      accountAction: {
        action: 'enable',
        accountNames: [newAccount.name],
      },
    };
  }

  private removeAccount(ctx: CommandContext): CommandResult {
    const removeAll = this.hasFlag(ctx, 'all');

    // Handle --all flag
    if (removeAll) {
      const accounts = ctx.adapter.listAccounts();

      if (accounts.length === 0) {
        this.printInfo(ctx, 'No accounts to remove');
        return this.success();
      }

      const count = accounts.length;
      for (const acc of accounts) {
        ctx.adapter.removeAccount(acc.name);
      }
      this.printSuccess(ctx, `Removed ${count} account(s)`);

      // Clear session since all accounts are gone
      return {
        success: true,
        updateSession: {
          exchange: null,
          accountName: undefined,
        },
      };
    }

    const name = this.getArg(ctx, 2);

    if (!name) {
      this.printWarning(ctx, 'Usage: accounts remove <name> or accounts remove --all');
      return this.success();
    }

    if (ctx.adapter.removeAccount(name)) {
      this.printSuccess(ctx, `Account "${name}" removed`);

      // If this was the current account, disconnect
      if (ctx.accountName === name) {
        return {
          success: true,
          updateSession: {
            exchange: null,
            accountName: undefined,
          },
        };
      }
    } else {
      this.printError(ctx, `Account "${name}" not found`);
    }

    return this.success();
  }

  private enableAccount(ctx: CommandContext): CommandResult {
    const enableAll = this.hasFlag(ctx, 'all');

    // Handle --all flag
    if (enableAll) {
      const accounts = ctx.adapter.listAccounts();
      const accountNames = accounts.map((acc) => acc.name);

      if (accountNames.length === 0) {
        this.printInfo(ctx, 'No accounts to enable');
        return this.success();
      }

      this.printInfo(ctx, `Enabling ${accountNames.length} account(s)...`);

      return {
        success: true,
        accountAction: {
          action: 'enable',
          accountNames: accountNames,
        },
      };
    }

    // Get all account names from args (starting at position 2)
    const accountNames = this.getAccountNamesFromArgs(ctx);

    if (accountNames.length === 0) {
      this.printWarning(ctx, 'Usage: accounts enable <name1> [name2] ... or accounts enable --all');
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

    // Return with action to enable the accounts
    return {
      success: true,
      accountAction: {
        action: 'enable',
        accountNames: validAccounts,
      },
    };
  }

  private disableAccount(ctx: CommandContext): CommandResult {
    const disableAll = this.hasFlag(ctx, 'all');

    // Handle --all flag - disable all accounts including primary
    if (disableAll) {
      const accounts = ctx.adapter.listAccounts();
      const accountNames = accounts.map((acc) => acc.name);

      if (accountNames.length === 0) {
        this.printInfo(ctx, 'No accounts to disable');
        return this.success();
      }

      this.printInfo(ctx, `Disabling ${accountNames.length} account(s)...`);

      return {
        success: true,
        accountAction: {
          action: 'disable',
          accountNames: accountNames,
        },
      };
    }

    // Get all account names from args (starting at position 2)
    const accountNames = this.getAccountNamesFromArgs(ctx);

    if (accountNames.length === 0) {
      this.printWarning(ctx, 'Usage: accounts disable <name1> [name2] ... or accounts disable --all');
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

    // Return with action to disable the accounts
    return {
      success: true,
      accountAction: {
        action: 'disable',
        accountNames: validAccounts,
      },
    };
  }

  private listEnabledAccounts(ctx: CommandContext): CommandResult {
    const accounts = ctx.adapter.listAccounts();

    // Filter to only connected/enabled accounts
    const enabledAccounts = accounts.filter((acc) => ctx.adapter.isAccountConnected?.(acc.name) ?? false);

    if (enabledAccounts.length === 0) {
      this.printInfo(ctx, 'No accounts currently enabled');
      return this.success();
    }

    this.print(ctx, ['', `Enabled Accounts (${enabledAccounts.length}):`, '']);
    enabledAccounts.forEach((acc, index) => {
      const isCurrent = acc.name === ctx.accountName;
      const num = String(index + 1).padStart(2, ' ');
      const suffix = isCurrent ? ' (primary)' : '';
      this.print(ctx, [`  ${num}. ${acc.name} - ${acc.exchange}${suffix}`]);
    });
    this.print(ctx, ['']);

    return this.success();
  }

  /**
   * Helper to get multiple account names from args (starting at position 2)
   */
  private getAccountNamesFromArgs(ctx: CommandContext): string[] {
    const names: string[] = [];
    for (let i = 2; i < ctx.args._.length; i++) {
      const name = ctx.args._[i];
      if (name && !names.includes(name)) {
        names.push(name);
      }
    }
    return names;
  }

  private showHelp(ctx: CommandContext): CommandResult {
    const brand = getCliBrand();
    this.print(ctx, [
      '',
      'Account Commands:',
      '',
      '  accounts              List all configured accounts',
      '  accounts enabled      List only enabled/streaming accounts',
      '  accounts add          Add a new account manually',
      `  accounts import <json> Import from ${brand.displayName} web export`,
      '  accounts remove <n>   Remove an account',
      '  accounts remove --all Remove all accounts',
      '  accounts enable <n>   Enable account streaming',
      '  accounts enable --all Enable all accounts',
      '  accounts disable <n>  Disable account streaming',
      '  accounts disable --all Disable all accounts',
      '  account <name>        Switch to an account',
      '  exchanges             List supported exchanges',
      '',
      'Multi-account streaming:',
      '  Enable accounts to stream data in the background.',
      '  The "primary" account is the one you switch to with "account <name>".',
      '  Enabled accounts show data in watch modes (watch account, etc.).',
      '',
      'Importing from web (recommended):',
      `  1. In ${brand.displayName} web, go to Settings > Accounts`,
      '  2. Click the export button on an account',
      '  3. Paste the JSON here: accounts import {"exchange":"bybit",...}',
      '',
      'Adding accounts manually:',
      '  accounts add -n <name> -e <exchange> -k <api-key> -s <api-secret> [-p <passphrase>] [-t]',
      '  accounts add -n <name> -e paradex -k <subkey-private-key> --paradex-address <addr> --subkey-address <addr>',
      `  Add ${brand.flags.proxy} [--proxy-region <region>] to match the web app's proxy routing option.`,
      '',
      'Options:',
      '  -n, --name           Account name (your choice)',
      '  -e, --exchange       Exchange (run "exchanges" to see list)',
      '  -k, --api-key        API key',
      '  -s, --api-secret     API secret',
      '  -p, --passphrase     Passphrase (OKX, BloFin, Bitget, KuCoin)',
      '  --private-key        Alias for wallet/private-key exchanges',
      '  --wallet-address     Alias for wallet/private-key exchanges',
      '  --application-id     Application ID / main wallet / account index',
      '  --api-key-index      API key index',
      '  --paradex-address    Paradex account address',
      '  --subkey-address     Paradex subkey public address',
      '  --omni-key-seed      Omni key seed',
      '  --subaccount         Exchange subaccount/account ID',
      '  --vault              Optional vault address/name',
      '  -t, --testnet        Use testnet',
      `  ${brand.flags.proxy.padEnd(20)} Route REST through the ${brand.displayName} proxy (IP-whitelisted keys)`,
      `  ${brand.flags.tunnel.padEnd(20)} Route REST through the ${brand.displayName} tradetunnel`,
      `  ${brand.flags.wsRoute.padEnd(20)} Route private WebSockets through ${brand.displayName}`,
      '  --proxy-region       Proxy region to use when proxy routing is enabled',
      '',
    ]);

    return this.success();
  }
}
