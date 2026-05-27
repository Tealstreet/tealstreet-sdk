import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { mergeUpdatesUsingLastUpdate } from './mergeUpdatesUsingLastUpdate';

vi.mock('./virtualClock', () => ({
  virtualClock: {
    getCurrentTimeMs: vi.fn(() => 10000),
  },
}));

describe('mergeUpdatesUsingLastUpdate', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('basic merging', () => {
    it('should return updates when existing is empty', () => {
      const updates = [
        { id: '1', value: 'update1' },
        { id: '2', value: 'update2' },
      ];
      const result = mergeUpdatesUsingLastUpdate(updates, []);
      expect(result).toHaveLength(2);
      expect(result).toEqual(updates);
    });

    it('should return existing when updates is empty', () => {
      const existing = [
        { id: '1', value: 'existing1', lastUpdate: 9500 },
        { id: '2', value: 'existing2', lastUpdate: 9500 },
      ];
      const result = mergeUpdatesUsingLastUpdate([], existing);
      expect(result).toHaveLength(2);
      expect(result).toEqual(existing);
    });

    it('should use update when existing has no lastUpdate', () => {
      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old' }];
      const result = mergeUpdatesUsingLastUpdate(updates, existing);
      expect(result).toHaveLength(1);
      expect(result[0].value).toBe('new');
    });
  });

  describe('lastUpdate cutoff logic', () => {
    it('should keep existing when lastUpdate is within cutoff (default 1000ms)', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old', lastUpdate: 9500 }]; // 500ms ago, within 1000ms cutoff
      const result = mergeUpdatesUsingLastUpdate(updates, existing);
      expect(result[0].value).toBe('old');
    });

    it('should use update when lastUpdate is beyond cutoff', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old', lastUpdate: 8000 }]; // 2000ms ago, beyond 1000ms cutoff
      const result = mergeUpdatesUsingLastUpdate(updates, existing);
      expect(result[0].value).toBe('new');
    });

    it('should use custom cutoff when provided', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old', lastUpdate: 9500 }]; // 500ms ago

      // With 200ms cutoff, should use update
      const result = mergeUpdatesUsingLastUpdate(updates, existing, 200);
      expect(result[0].value).toBe('new');
    });

    it('should keep existing with larger custom cutoff', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old', lastUpdate: 8000 }]; // 2000ms ago

      // With 5000ms cutoff, should keep existing
      const result = mergeUpdatesUsingLastUpdate(updates, existing, 5000);
      expect(result[0].value).toBe('old');
    });

    it('should keep existing when exactly at cutoff boundary', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old', lastUpdate: 9000 }]; // exactly 1000ms ago

      // NOTE: Uses > (not >=), so exactly at boundary keeps existing
      const result = mergeUpdatesUsingLastUpdate(updates, existing, 1000);
      expect(result[0].value).toBe('old');
    });

    it('should use update when just past cutoff boundary', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old', lastUpdate: 8999 }]; // 1001ms ago

      const result = mergeUpdatesUsingLastUpdate(updates, existing, 1000);
      expect(result[0].value).toBe('new');
    });
  });

  describe('mixed updates and existing', () => {
    it('should handle mix of matching and non-matching items', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [
        { id: '1', value: 'update1' },
        { id: '3', value: 'update3' },
      ];
      const existing = [
        { id: '1', value: 'existing1', lastUpdate: 9500 }, // Should keep (within cutoff)
        { id: '2', value: 'existing2', lastUpdate: 9500 }, // Should remain (not in updates)
      ];

      const result = mergeUpdatesUsingLastUpdate(updates, existing);

      expect(result).toHaveLength(3);
      expect(result.find((r) => r.id === '1')?.value).toBe('existing1');
      expect(result.find((r) => r.id === '2')?.value).toBe('existing2');
      expect(result.find((r) => r.id === '3')?.value).toBe('update3');
    });

    it('should deduplicate by id using uniqBy', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'update' }];
      const existing = [{ id: '1', value: 'existing', lastUpdate: 8000 }]; // Beyond cutoff

      const result = mergeUpdatesUsingLastUpdate(updates, existing);

      // Should only have one item with id '1'
      expect(result.filter((r) => r.id === '1')).toHaveLength(1);
    });
  });

  describe('edge cases', () => {
    it('should handle empty arrays', () => {
      const result = mergeUpdatesUsingLastUpdate([], []);
      expect(result).toEqual([]);
    });

    it('should handle existing item with lastUpdate of 0', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [{ id: '1', value: 'new' }];
      const existing = [{ id: '1', value: 'old', lastUpdate: 0 }]; // 10000ms ago

      const result = mergeUpdatesUsingLastUpdate(updates, existing);
      expect(result[0].value).toBe('new');
    });

    it('should handle multiple updates for same id', async () => {
      const { virtualClock } = await import('./virtualClock');
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(10000);

      const updates = [
        { id: '1', value: 'first' },
        { id: '1', value: 'second' },
      ];
      const existing: { id: string; value: string; lastUpdate?: number }[] = [];

      const result = mergeUpdatesUsingLastUpdate(updates, existing);
      // uniqBy keeps first occurrence
      expect(result).toHaveLength(1);
      expect(result[0].value).toBe('first');
    });
  });
});
