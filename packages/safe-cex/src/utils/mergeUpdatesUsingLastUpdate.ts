import uniqBy from 'lodash/uniqBy';
import { virtualClock } from './virtualClock';

/**
 * Returns a new array merging updates with existing, picking existing item if existing.lastUpdate is within timeCutoff
 * else picking the update. Useful for combining updates from rest api with ws updates as rest often is slightly delayed / cached.
 * @param updates
 * @param existing
 * @param existingLastUpdateCutoff
 */
export const mergeUpdatesUsingLastUpdate = <T extends { lastUpdate?: number; id: string }>(
  updates: T[],
  existing: T[],
  existingLastUpdateCutoff: number = 1000,
): T[] => {
  const now = virtualClock.getCurrentTimeMs();
  const safeUpdates = updates.map((update) => {
    const foundExisting = existing.find((e) => e.id === update.id);
    if (!foundExisting?.lastUpdate) {
      return update;
    }
    if (now - foundExisting.lastUpdate > existingLastUpdateCutoff) {
      return update;
    }
    return foundExisting;
  });

  return uniqBy([...safeUpdates, ...existing], 'id');
};
