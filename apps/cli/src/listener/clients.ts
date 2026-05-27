import type { StandalonePlatformAdapter } from '../platform-adapter';
import { loadTrustedClients, removeTrustedClient, removeAllTrustedClients } from './state';

export function cmdClientsList(adapter: StandalonePlatformAdapter): void {
  const { clients } = loadTrustedClients();
  if (clients.length === 0) {
    adapter.printInfo('No trusted clients.');
    return;
  }
  adapter.printInfo(`Trusted clients (${clients.length}):`);
  for (const c of clients) {
    const when = new Date(c.approvedAt).toISOString().slice(0, 19).replace('T', ' ');
    adapter.print([`  ${c.clientId.slice(0, 8)}…  ${c.origin}  ${c.label}  (approved ${when})`]);
  }
}

export function cmdClientsRevoke(adapter: StandalonePlatformAdapter, target: string | undefined): void {
  if (!target) {
    adapter.printError('Usage: clients revoke <id|prefix|--all>');
    return;
  }
  if (target === '--all') {
    const n = removeAllTrustedClients();
    adapter.printSuccess(`Revoked ${n} client(s).`);
    return;
  }
  if (target.length < 8) {
    adapter.printError('Prefix must be at least 8 characters (or pass the full id).');
    return;
  }
  const { removed, matches } = removeTrustedClient(target);
  if (removed.length === 1) {
    adapter.printSuccess(`Revoked ${removed[0].clientId}.`);
    return;
  }
  if (matches && matches.length > 1) {
    adapter.printError(`Ambiguous prefix matches ${matches.length} clients:`);
    for (const m of matches) {
      adapter.print([`  ${m.clientId}  ${m.label}`]);
    }
    return;
  }
  adapter.printError(`No client matches "${target}".`);
}
