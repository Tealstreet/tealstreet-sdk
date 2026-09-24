import type { AccountConfig, CLIPlatformAdapter, CLISession } from './types';

/**
 * Connect to an account.
 *
 * Kept here because the standalone REPL imports it directly. The old session
 * command parser that also lived in this file was removed so account add/import
 * flows go through the registered AccountsCommand only.
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

    // A venue that rejected the key still "starts" — `start()` has no auth gate
    // — so claiming success here is how a 401 ended up reading as a connected
    // account with an empty balance.
    if ((exchange.authFailureStreak ?? 0) > 0) {
      adapter.printError(`${account.exchange}: connected, but the exchange rejected this API key.`);
    } else {
      adapter.printSuccess(`Connected to ${account.exchange}`);
    }

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
