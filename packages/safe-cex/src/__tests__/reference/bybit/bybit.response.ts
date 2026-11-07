type BybitEnvelopeLike = Record<string, any>;

const normalizeMessage = (value: unknown): string | undefined => {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
};

const normalizeCode = (value: unknown): number | undefined => {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }

  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (trimmed.length === 0) return undefined;
    const parsed = Number(trimmed);
    return Number.isFinite(parsed) ? parsed : undefined;
  }

  return undefined;
};

const parseRawResponse = (value: unknown): BybitEnvelopeLike | undefined => {
  if (!value) return undefined;
  if (typeof value === 'object' && !Array.isArray(value)) {
    return value as BybitEnvelopeLike;
  }
  if (typeof value !== 'string') return undefined;

  try {
    const parsed = JSON.parse(value);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as BybitEnvelopeLike;
    }
  } catch {
    return undefined;
  }

  return undefined;
};

const firstDefinedCode = (...candidates: unknown[]): number | undefined => {
  for (const candidate of candidates) {
    const normalized = normalizeCode(candidate);
    if (normalized !== undefined) {
      return normalized;
    }
  }
  return undefined;
};

const firstDefinedMessage = (...candidates: unknown[]): string | undefined => {
  for (const candidate of candidates) {
    const normalized = normalizeMessage(candidate);
    if (normalized !== undefined) {
      return normalized;
    }
  }
  return undefined;
};

export type BybitResponseMeta = {
  isSuccess: boolean;
  hasExplicitStatus: boolean;
  retCode?: number;
  retMsg?: string;
  rawResponse?: BybitEnvelopeLike;
};

export const getBybitResponseMeta = (payload: unknown): BybitResponseMeta => {
  const envelope = (payload && typeof payload === 'object' ? payload : {}) as BybitEnvelopeLike;
  const rawResponse = parseRawResponse(envelope.raw_response ?? envelope.rawResponse);

  const retCode = firstDefinedCode(
    envelope.retCode,
    envelope.ret_code,
    envelope.code,
    rawResponse?.retCode,
    rawResponse?.ret_code,
    rawResponse?.code,
  );

  const retMsg = firstDefinedMessage(
    envelope.retMsg,
    envelope.ret_msg,
    envelope.message,
    envelope.msg,
    rawResponse?.retMsg,
    rawResponse?.ret_msg,
    rawResponse?.message,
    rawResponse?.msg,
  );

  const hasExplicitStatus = retCode !== undefined || retMsg !== undefined;
  const isSuccess = retCode !== undefined ? retCode === 0 : retMsg === 'OK';

  return {
    isSuccess,
    hasExplicitStatus,
    retCode,
    retMsg,
    rawResponse,
  };
};
