import { BybitExchange } from './bybit.exchange';

export class BybitDemoExchange extends BybitExchange {
  isDemo = true;

  get supportedCategories() {
    return ['linear'];
  }

  protected optionsEnabled() {
    return false;
  }

  protected inverseEnabled() {
    return false;
  }
}
