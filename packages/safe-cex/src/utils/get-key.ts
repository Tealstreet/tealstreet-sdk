import camelCase from 'lodash/camelCase';
import memoize from 'lodash/memoize';
import snakeCase from 'lodash/snakeCase';

const s = memoize(snakeCase);
const c = memoize(camelCase);

export const v = (obj: Record<string, any>, key: string) => {
  if (obj == null) return undefined;
  if (key in obj) return obj[key];
  if (s(key) in obj) return obj[s(key)];
  if (c(key) in obj) return obj[c(key)];
  return undefined;
};

export const vn = (obj: Record<string, any>, ...keys: string[]) => {
  for (const key of keys) {
    const val = v(obj, key);
    if (val !== undefined) return val;
  }
  return undefined;
};
