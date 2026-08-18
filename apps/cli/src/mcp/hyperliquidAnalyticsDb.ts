import { Pool } from 'pg';

type HyperliquidAnalyticsConfig =
  | { kind: 'connection-string'; connectionString: string }
  | { kind: 'parts'; host: string; database: string; user: string; password: string; port: number };

let pool: Pool | null | undefined;

const config = (): HyperliquidAnalyticsConfig | null => {
  const connectionString = process.env.HYPERLIQUID_ANALYTICS_DATABASE_URL;
  if (connectionString) return { kind: 'connection-string', connectionString };

  const host = process.env.HYPERLIQUID_ANALYTICS_POSTGRES_HOSTNAME;
  const database = process.env.HYPERLIQUID_ANALYTICS_POSTGRES_DATABASE;
  const user = process.env.HYPERLIQUID_ANALYTICS_POSTGRES_USERNAME;
  const password = process.env.HYPERLIQUID_ANALYTICS_POSTGRES_PASSWORD;
  const port = Number(process.env.HYPERLIQUID_ANALYTICS_POSTGRES_PORT ?? 5432);

  if (!host || !database || !user || !password || !Number.isFinite(port)) return null;
  return { kind: 'parts', host, database, user, password, port };
};

const getPool = (): Pool | null => {
  if (pool !== undefined) return pool;

  const resolved = config();
  if (!resolved) {
    pool = null;
    return pool;
  }

  pool =
    resolved.kind === 'connection-string'
      ? new Pool({ connectionString: resolved.connectionString, max: 2 })
      : new Pool({
          host: resolved.host,
          database: resolved.database,
          user: resolved.user,
          password: resolved.password,
          port: resolved.port,
          max: 2,
        });

  return pool;
};

export const queryHyperliquidAnalytics = async <T extends Record<string, unknown>>(
  sql: string,
  params: unknown[] = [],
): Promise<{ ok: true; rows: T[] } | { ok: false; reason: 'not-configured' }> => {
  const analyticsPool = getPool();
  if (!analyticsPool) return { ok: false, reason: 'not-configured' };

  const result = await analyticsPool.query<T>(sql, params);
  return { ok: true, rows: result.rows };
};
