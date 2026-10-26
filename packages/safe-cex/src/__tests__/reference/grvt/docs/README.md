# GRVT API Documentation

This directory contains cached copies of the official GRVT (Gravity) API documentation from their GitHub repository.

## Source

These files are sourced from:
**https://github.com/gravity-technologies/api-spec/tree/main/artifacts/apidocs**

## Files

- **market_data_api.md** - REST API endpoints for market data (instruments, tickers, orderbook, trades, klines)
- **market_data_streams.md** - WebSocket streams for real-time market data
- **trading_api.md** - REST API endpoints for trading operations (orders, positions, account management)
- **trading_streams.md** - WebSocket streams for real-time trading updates

## Purpose

These files are cached locally because:
1. The official docs at https://api-docs.grvt.io/ may return 403 errors when accessed programmatically
2. Having local copies ensures documentation is always available during development
3. Version control allows tracking changes to the API specification over time

## Updates

To update these files to the latest version:

```bash
cd packages/safe-cex/src/exchanges/grvt/docs

curl -s https://raw.githubusercontent.com/gravity-technologies/api-spec/main/artifacts/apidocs/market_data_api.md -o market_data_api.md
curl -s https://raw.githubusercontent.com/gravity-technologies/api-spec/main/artifacts/apidocs/market_data_streams.md -o market_data_streams.md
curl -s https://raw.githubusercontent.com/gravity-technologies/api-spec/main/artifacts/apidocs/trading_api.md -o trading_api.md
curl -s https://raw.githubusercontent.com/gravity-technologies/api-spec/main/artifacts/apidocs/trading_streams.md -o trading_streams.md
```

## License

These documentation files are sourced from GRVT's public GitHub repository and remain under their original license terms.
