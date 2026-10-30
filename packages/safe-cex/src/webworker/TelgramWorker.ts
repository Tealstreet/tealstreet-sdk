import axios from 'axios';

import { Exchange } from '../exchanges/base';
import { PositionSide } from '../types';
import { rustAlgoExchange } from './handlers/BaseHandler';

type TelegramMessage = { message: { chat: { id: number }; text: string }; update_id: number };

export class TelegramWorker {
  private _token: string;
  private _tickCounter = 0;
  private _seenMessages = new Set<number>();
  private _getExchange: () => Exchange;

  private get _exchange() {
    return this._getExchange();
  }

  constructor(getExchange: () => Exchange, telegramBotToken: string) {
    this._getExchange = getExchange;
    this._token = telegramBotToken;
  }

  start() {
    this.tick();
  }

  async processMessages(messages: TelegramMessage[]) {
    for (const message of messages) {
      if (this._seenMessages.has(message.update_id)) {
        continue;
      }
      if (message.message.text === '/positions') {
        axios.post(`https://api.telegram.org/bot${this._token}/sendMessage`, {
          chat_id: message.message.chat.id,
          text: this._exchange.store.positions
            .map(
              (position) =>
                `${position.symbol} ${position.side === PositionSide.Long ? 'long' : 'short'} UPNL: ${position.unrealizedPnl}`,
            )
            .join('\n'),
        });
      } else if (message.message.text.startsWith('/cli ')) {
        // Format: /cli SYMBOL command args
        // Example: /cli BTCUSDT buy 0.01
        const parts = message.message.text.replace(/^\/cli\s+/, '').split(/\s+/);
        const symbol = parts[0] || '';
        const command = parts.slice(1).join(' ');

        if (!symbol || !command) {
          axios.post(`https://api.telegram.org/bot${this._token}/sendMessage`, {
            chat_id: message.message.chat.id,
            text: 'Usage: /cli SYMBOL command [args]\nExample: /cli BTCUSDT buy 0.01',
          });
          continue;
        }

        // The command runs in the account engine's Rust algos, as the terminal's `exec` does.
        const exchange = rustAlgoExchange(this._exchange);
        if (exchange) exchange.algoOp('exec', { symbol, command });
        else
          this._exchange.emitter.emit('exec-log', {
            message: 'Command failed: algos run only on a Rust engine account.',
          });
      }
    }
  }

  async tick() {
    const resp = await axios.get(`https://api.telegram.org/bot${this._token}/getUpdates`);
    this._tickCounter++;
    const messages = resp.data.result as TelegramMessage[];
    // warm up seen messages caches after worker restart for ~ 5s
    if (this._tickCounter < 10) {
      for (const message of messages) {
        this._seenMessages.add(message.update_id);
      }
    } else {
      this.processMessages(messages);
      for (const message of messages) {
        this._seenMessages.add(message.update_id);
      }
    }
    setTimeout(this.tick.bind(this), 1_000);
  }
}
