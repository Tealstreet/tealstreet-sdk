import type { StandalonePlatformAdapter } from '../platform-adapter';

const APPROVAL_TIMEOUT_MS = 30000;

export async function promptApproval({
  adapter,
  clientId,
  label,
  origin,
  timeoutMs = APPROVAL_TIMEOUT_MS,
}: {
  adapter: StandalonePlatformAdapter;
  clientId: string;
  label: string;
  origin: string;
  timeoutMs?: number;
}): Promise<boolean> {
  if (!adapter.promptYesNo) return false;
  const trimmedLabel = label.slice(0, 80);
  const message =
    `\nNew client wants to connect:\n` +
    `  origin: ${origin}\n` +
    `  id:     ${clientId}\n` +
    `  label:  ${trimmedLabel}\n` +
    `This client will be able to read your API keys, sync accounts,\n` +
    `and execute trades on your behalf.\n` +
    `Approve? [y/N] `;
  return adapter.promptYesNo(message, { timeoutMs });
}
