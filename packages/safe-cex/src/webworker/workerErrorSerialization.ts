export type SerializableWorkerError = {
  message: string;
  name: string;
  stack?: string;
};

const readStringProperty = (value: unknown, key: string): string | undefined => {
  if (!value || typeof value !== 'object') return undefined;
  const property = (value as Record<string, unknown>)[key];
  return typeof property === 'string' && property.length > 0 ? property : undefined;
};

export const serializeWorkerErrorForSentry = (
  error: unknown,
  fallbackMessage: string,
  fallbackName = 'Error',
): SerializableWorkerError => {
  const serialized: SerializableWorkerError = {
    message: readStringProperty(error, 'message') ?? fallbackMessage,
    name: readStringProperty(error, 'name') ?? fallbackName,
  };
  const stack = readStringProperty(error, 'stack');
  if (stack) serialized.stack = stack;
  return serialized;
};
