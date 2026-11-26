import omitBy from 'lodash/omitBy';

export const omitUndefined = <T extends object>(obj: T): T => {
  return omitBy<T>(obj, (value) => value === undefined) as T;
};
