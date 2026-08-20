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
