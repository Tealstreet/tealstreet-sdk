/**
 * What an interactive start connects, primary first. An account named on the
 * command line connects even when inactive, as its "Connecting" line promises.
 */
export function accountsToStart<A extends { name: string; active?: boolean }>(
  primary: A | null | undefined,
  namedOnCommandLine: boolean,
  activeAccounts: readonly A[],
  isConnected: (name: string) => boolean,
): A[] {
  const accounts: A[] = [];
  if (primary && (primary.active || namedOnCommandLine) && !isConnected(primary.name)) accounts.push(primary);
  for (const account of activeAccounts) {
    if (account.name === primary?.name || isConnected(account.name)) continue;
    accounts.push(account);
  }
  return accounts;
}
