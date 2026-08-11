import { OKXExchange } from './okx.exchange';
import { ENDPOINTS } from './okx.types';

export class OKXEUExchange extends OKXExchange {
  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.ACCOUNT_CONFIG);
      return { error: '' };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || err?.message };
    }
  };
}
