import { KeyValueCandleCacheBackend } from '../../../safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend';

type RedisScanResult = [string, string[]] | { cursor: string; keys: string[] };

export interface RedisLikeCandleCacheClient {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<unknown>;
  del(...keys: string[]): Promise<unknown>;
  keys?(pattern: string): Promise<string[]>;
  scan?(cursor: string, ...args: string[]): Promise<RedisScanResult>;
}

export class RedisCandleCacheBackend extends KeyValueCandleCacheBackend {
  constructor(client: RedisLikeCandleCacheClient, namespace?: string) {
    super(
      {
        get: (key) => client.get(key),
        set: async (key, value) => {
          await client.set(key, value);
        },
        delete: async (key) => {
          await client.del(key);
        },
        listKeys: async (prefix) => {
          if (client.scan) {
            const keys: string[] = [];
            let cursor = '0';
            do {
              const result = await client.scan(cursor, 'MATCH', `${prefix}*`, 'COUNT', '500');
              if (Array.isArray(result)) {
                cursor = result[0];
                keys.push(...result[1]);
              } else {
                cursor = result.cursor;
                keys.push(...result.keys);
              }
            } while (cursor !== '0');

            return keys;
          }

          if (client.keys) {
            return client.keys(`${prefix}*`);
          }

          throw new Error('Redis client must implement scan() or keys() for candle cache key discovery');
        },
      },
      namespace,
    );
  }
}
