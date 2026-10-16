import { KeyValueCandleCacheBackend } from '../../../safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend';

export interface AsyncStorageLike {
  getItem(key: string): Promise<string | null>;
  setItem(key: string, value: string): Promise<void>;
  removeItem(key: string): Promise<void>;
  getAllKeys(): Promise<string[]>;
}

export class ReactNativeAsyncStorageCandleCacheBackend extends KeyValueCandleCacheBackend {
  constructor(storage: AsyncStorageLike, namespace?: string) {
    super(
      {
        get: (key) => storage.getItem(key),
        set: (key, value) => storage.setItem(key, value),
        delete: (key) => storage.removeItem(key),
        listKeys: async (prefix) => {
          const keys = await storage.getAllKeys();
          return keys.filter((key) => key.startsWith(prefix));
        },
      },
      namespace,
    );
  }
}
