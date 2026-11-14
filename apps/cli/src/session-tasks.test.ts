import { expect, it, vi } from 'vitest';
const tasks = vi.hoisted(() => ({
  chases: [{ id: 'fixture-A' }],
  remove: vi.fn(),
  clearTriggers: vi.fn(),
  twaps: new Map([['fixture-A', { terminate: vi.fn(async () => {}) }]]),
}));
vi.mock('@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager', () => ({
  chaserOrderManagerInstance: { instances: tasks.chases, removeKeepOrder: tasks.remove },
}));
vi.mock('@tealstreet/safe-cex/src/webworker/twap/TwapManager', () => ({ twapManager: { runningTwaps: tasks.twaps } }));
vi.mock('@tealstreet/cli-core/src/triggers/triggerRegistry', () => ({ clearTriggers: tasks.clearTriggers }));
it('stops prior-session supervisors, clears queued triggers and detaches TWAPs on login change', async () => {
  const { selectStorageOwner } = await import('./storage-context');
  selectStorageOwner('A');
  await import('./session-tasks');
  const twap = tasks.twaps.get('fixture-A');
  selectStorageOwner('B');
  expect(tasks.remove).toHaveBeenCalledWith('fixture-A');
  expect(tasks.clearTriggers).toHaveBeenCalledOnce();
  expect(twap?.terminate).toHaveBeenCalledOnce();
  expect(tasks.twaps.size).toBe(0);
});
