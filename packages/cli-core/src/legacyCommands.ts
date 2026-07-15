import type { CLIPlatformAdapter, CLISession, CommandResult, AccountConfig, ExchangeName } from './types';
import { getSupportedExchangeNames, getExchangeInfo } from './exchanges';

/**
 * Parse command line arguments for config add
 */
function parseConfigAddArgs(argsString: string): Record<string, string | boolean> {
  const args = argsString.split(/\s+/);
  const parsed: Record<string, string | boolean> = {};

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '-n' || arg === '--name') parsed.name = args[++i];
    else if (arg === '-e' || arg === '--exchange') parsed.exchange = args[++i];
    else if (arg === '-k' || arg === '--api-key') parsed.apiKey = args[++i];
    else if (arg === '-s' || arg === '--api-secret') parsed.apiSecret = args[++i];
    else if (arg === '-p' || arg === '--passphrase') parsed.passphrase = args[++i];
    else if (arg === '--private-key') parsed.privateKey = args[++i];
    else if (arg === '--wallet-address') parsed.walletAddress = args[++i];
    else if (arg === '-t' || arg === '--testnet') parsed.testnet = true;
  }

  return parsed;
}

/**
 * Session commands that are handled by cli-core
 * These work independently of trading commands and don't require exchange connection
 */
const SESSION_COMMANDS = [
  'help',
  'status',
  'symbol',
  'markets',
  'config',
  'exchanges',
  'accounts',
  'account',
];

/**
 * Check if a command is a session command handled by cli-core
 */
export function isSessionCommand(command: string): boolean {
  const trimmedCommand = command.trim();
  const mainCommand = trimmedCommand.split(' ')[0];
  return SESSION_COMMANDS.includes(mainCommand);
}

/**
 * Process session commands - shared logic between web and standalone
 * Returns { handled: false } for trading commands that should be passed to commandRouter
 */
export async function processSessionCommand(
  command: string,
  session: CLISession,
  adapter: CLIPlatformAdapter,
): Promise<CommandResult> {
  const trimmedCommand = command.trim();

  if (!trimmedCommand) {
    return { handled: true };
  }

  // Help command - works without connection
  if (trimmedCommand === 'help') {
    adapter.print([
      '',
      'Tealstreet CLI Commands:',
      '',
      'Accounts:',
      '  accounts           List configured accounts',
      '  accounts add       Add a new account',
      '  accounts remove    Remove an account',
      '  account <name>     Switch to account',
      '  exchanges          List supported exchanges',
      '',
      'Session:',
      '  symbol <symbol>    Change trading symbol',
      '  markets            List available markets',
      '  status             Show current connection status',
      '  exit               Exit the CLI',
      '',
      'Trading:',
      '  buy                Buy market/limit order',
      '  sell               Sell market/limit order',
      '  close              Close position',
      '  cancel             Cancel orders',
      '  scale              Scale into position',
      '  chase              Chase order',
      '  stop               Place stop order',
      '  nuke               Cancel all & close all',
      '  balance            Show balance',
      '',
      'Type "accounts help" for account setup details',
      'Type "help <command>" for trading command details',
      '',
    ]);
    return { handled: true };
  }

  // Show current status
  if (trimmedCommand === 'status') {
    adapter.printInfo(`Account: ${session.currentAccountName || 'Not connected'}`);
    adapter.printInfo(`Exchange: ${session.currentExchange?.name || 'Not connected'}`);
    adapter.printInfo(`Symbol: ${session.currentSymbol}`);
    return { handled: true };
  }

  // Handle symbol switching
  if (trimmedCommand.startsWith('symbol ')) {
    const newSymbol = trimmedCommand.split(' ')[1]?.toUpperCase();
    if (newSymbol) {
      adapter.setDefaultSymbol(newSymbol);
      adapter.printInfo(`Symbol set to ${newSymbol}`);
      return {
        handled: true,
        sessionChanged: true,
        newSession: { currentSymbol: newSymbol },
      };
    }
    return { handled: true };
  }

  // List available markets
  if (trimmedCommand === 'markets') {
    if (!session.currentExchange) {
      adapter.printError('Not connected to any exchange');
      return { handled: true };
    }
    const markets = session.currentExchange.store?.markets || [];
    const symbols = markets.map((m) => m.symbol).sort();
    adapter.printInfo(`Available markets (${markets.length}):`);
    // Print in columns
    const cols = 4;
    for (let i = 0; i < symbols.length; i += cols) {
      adapter.print(['  ' + symbols.slice(i, i + cols).join('\t')]);
    }
    return { handled: true };
  }

  // Config commands
  if (trimmedCommand === 'config exchanges' || trimmedCommand === 'exchanges') {
    adapter.print(['', 'Supported Exchanges:', '']);
    for (const name of getSupportedExchangeNames()) {
      const info = getExchangeInfo(name);
      if (info) {
        adapter.print([`  ${name.padEnd(15)} ${info.displayName}`]);
        adapter.printInfo(`                 Required: ${info.requiredFields.join(', ')}`);
      }
    }
    adapter.print(['']);
    return { handled: true };
  }

  // accounts (list) - show all configured accounts
  if (trimmedCommand === 'accounts' || trimmedCommand === 'accounts list' || trimmedCommand === 'config list') {
    const accounts = adapter.listAccounts();
    if (accounts.length === 0) {
      adapter.printWarning('No accounts configured.');
      adapter.printInfo('Use "accounts add" to add an account.');
    } else {
      adapter.print(['', 'Configured Accounts:', '']);
      accounts.forEach((acc) => {
        const isCurrent = acc.name === session.currentAccountName;
        const prefix = isCurrent ? '> ' : '  ';
        const suffix = isCurrent ? ' (connected)' : '';
        adapter.print([`${prefix}${acc.name} - ${acc.exchange}${suffix}`]);
      });
      adapter.print(['']);
    }
    return { handled: true };
  }

  // accounts add -n name -e exchange -k key -s secret [-p passphrase] [--private-key key] [--wallet-address addr] [-t]
  if (
    trimmedCommand === 'accounts add' ||
    trimmedCommand.startsWith('accounts add ') ||
    trimmedCommand === 'config add' ||
    trimmedCommand.startsWith('config add ')
  ) {
    // Normalize to get args
    const isAccountsCmd = trimmedCommand.startsWith('accounts');
    const prefix = isAccountsCmd ? 'accounts add' : 'config add';

    // If just "accounts add" with no args, show usage
    if (trimmedCommand === 'accounts add' || trimmedCommand === 'config add') {
      adapter.printWarning('Usage: accounts add -n <name> -e <exchange> -k <api-key> -s <api-secret> [-p <passphrase>] [-t]');
      adapter.printInfo('       accounts add -n <name> -e <exchange> --private-key <key> --wallet-address <addr> [-t]');
      adapter.printInfo('Use "accounts help" to see all options.');
      return { handled: true };
    }

    const argsString = trimmedCommand.slice(prefix.length + 1);
    const parsed = parseConfigAddArgs(argsString);

    if (!parsed.name || !parsed.exchange) {
      adapter.printWarning('Usage: accounts add -n <name> -e <exchange> -k <api-key> -s <api-secret> [-p <passphrase>] [-t]');
      adapter.printInfo('       accounts add -n <name> -e <exchange> --private-key <key> --wallet-address <addr> [-t]');
      return { handled: true };
    }

    const exchangeInfo = getExchangeInfo(parsed.exchange as string);
    if (!exchangeInfo) {
      adapter.printError(`Unknown exchange: ${parsed.exchange}`);
      adapter.printInfo('Use "exchanges" to see supported exchanges');
      return { handled: true };
    }

    const newAccount: AccountConfig = {
      name: parsed.name as string,
      exchange: (parsed.exchange as string).toLowerCase() as ExchangeName,
      apiKey: (parsed.privateKey as string) || (parsed.apiKey as string) || '',
      apiSecret: (parsed.apiSecret as string) || '',
      passphrase: parsed.passphrase as string | undefined,
      testnet: (parsed.testnet as boolean) || false,
    };

    adapter.addAccount(newAccount);
    adapter.printSuccess(`Account "${newAccount.name}" added successfully`);

    return { handled: true };
  }

  // accounts remove <name>
  if (
    trimmedCommand.startsWith('accounts remove ') ||
    trimmedCommand.startsWith('accounts rm ') ||
    trimmedCommand.startsWith('config remove ') ||
    trimmedCommand.startsWith('config rm ')
  ) {
    const name = trimmedCommand.split(' ').pop();
    if (name && adapter.removeAccount(name)) {
      adapter.printSuccess(`Account "${name}" removed`);
      if (session.currentAccountName === name) {
        return {
          handled: true,
          sessionChanged: true,
          newSession: {
            currentExchange: null,
            currentAccountName: undefined,
          },
        };
      }
    } else {
      adapter.printError(`Account "${name}" not found`);
    }
    return { handled: true };
  }

  // accounts help / config help
  if (trimmedCommand === 'accounts help' || trimmedCommand === 'config' || trimmedCommand === 'config help') {
    adapter.print([
      '',
      'Account Commands:',
      '',
      '  accounts             List all configured accounts',
      '  accounts add         Add a new account',
      '  accounts remove <n>  Remove an account',
      '  account <name>       Switch to an account',
      '  exchanges            List supported exchanges',
      '',
      'Adding accounts:',
      '  accounts add -n <name> -e <exchange> -k <api-key> -s <api-secret> [-p <passphrase>] [-t]',
      '  accounts add -n <name> -e <exchange> --private-key <key> --wallet-address <addr> [-t]',
      '',
      'Options:',
      '  -n, --name           Account name (your choice)',
      '  -e, --exchange       Exchange (run "exchanges" to see list)',
      '  -k, --api-key        API key',
      '  -s, --api-secret     API secret',
      '  -p, --passphrase     Passphrase (OKX, BloFin, Bitget, KuCoin)',
      '  --private-key        Private key (Hyperliquid, Paradex)',
      '  --wallet-address     Wallet address (for DEX exchanges)',
      '  -t, --testnet        Use testnet',
      '',
    ]);
    return { handled: true };
  }

  // Handle account switching
  if (trimmedCommand.startsWith('account ')) {
    const newAccountName = trimmedCommand.split(' ')[1];
    if (newAccountName) {
      const newAccount = adapter.getAccount(newAccountName);
      if (newAccount) {
        adapter.printInfo(`Switching to ${newAccount.exchange}...`);
        if (session.currentExchange) {
          await adapter.stopExchange(session.currentExchange);
        }
        try {
          const newExchange = await adapter.createExchange(newAccount);
          await adapter.startExchange(newExchange);
          adapter.printSuccess(`Connected to ${newAccount.exchange}`);
          return {
            handled: true,
            sessionChanged: true,
            newSession: {
              currentExchange: newExchange,
              currentAccountName: newAccount.name,
            },
          };
        } catch (error) {
          adapter.printError(`Failed to connect: ${(error as Error).message}`);
        }
      } else {
        adapter.printError(`Account "${newAccountName}" not found`);
      }
    }
    return { handled: true };
  }

  // Not a session command - let the platform handle it (trading commands)
  return { handled: false };
}

/**
 * Connect to an account
 */
export async function connectToAccount(
  account: AccountConfig,
  session: CLISession,
  adapter: CLIPlatformAdapter,
): Promise<CLISession> {
  adapter.printInfo(`Connecting to ${account.exchange}...`);

  try {
    if (session.currentExchange) {
      await adapter.stopExchange(session.currentExchange);
    }

    const exchange = await adapter.createExchange(account);
    await adapter.startExchange(exchange);

    adapter.printSuccess(`Connected to ${account.exchange}`);

    return {
      ...session,
      currentExchange: exchange,
      currentAccountName: account.name,
    };
  } catch (error) {
    adapter.printError(`Failed to connect: ${(error as Error).message}`);
    return session;
  }
}
