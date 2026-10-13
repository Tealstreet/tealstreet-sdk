import type { AxiosError, AxiosInstance, AxiosResponse, InternalAxiosRequestConfig } from 'axios';

// One re-signed resend on a signature rejection: a refusal, so nothing executed and
// writes are as safe as reads. The venue's signer must sign what a resend sends —
// `config.data` is already a string, and the first pass's signature is still on it.

export const SIGNATURE_RETRY_CONFIG_KEY = 'safeCexSignatureRetry';

/** Reads a venue's error body and says whether it is a signature rejection. */
export type IsSignatureRejection = (data: any) => boolean;

/** Binance's -1022, which the venues that copied its API (Bitrue, AsterDEX) kept. */
export const isBinanceStyleSignatureRejection: IsSignatureRejection = (data) =>
  Number(data?.code) === -1022 || /signature for this request is not valid/i.test(String(data?.msg ?? ''));

export const hasRetriedSignature = (config: unknown): boolean =>
  Boolean((config as Record<string, unknown> | undefined)?.[SIGNATURE_RETRY_CONFIG_KEY]);

/** axios-retry condition for a non-2xx signature rejection. Stamps the config it
 * approves; axios-retry resends that config, so a second rejection surfaces. */
export const signatureRejectedResponse =
  (isRejection: IsSignatureRejection) =>
  (error: AxiosError): boolean => {
    const config = error.config as InternalAxiosRequestConfig | undefined;
    if (!config || !error.response || hasRetriedSignature(config)) return false;
    if (!isRejection(error.response.data)) return false;
    (config as unknown as Record<string, unknown>)[SIGNATURE_RETRY_CONFIG_KEY] = true;
    return true;
  };

/** For a rejection axios resolves (a 200 envelope, or any status over the HOW tunnel),
 * which axios-retry never sees. Call it in the venue's LAST response interceptor before
 * any side effect: the resend already ran the whole chain, so later ones would run twice. */
export const resendOnSignatureRejection = (
  xhr: AxiosInstance,
  response: AxiosResponse,
  isRejection: IsSignatureRejection,
): Promise<AxiosResponse> | undefined => {
  if (hasRetriedSignature(response.config) || !isRejection(response.data)) return undefined;
  return xhr.request({ ...response.config, [SIGNATURE_RETRY_CONFIG_KEY]: true } as InternalAxiosRequestConfig);
};
