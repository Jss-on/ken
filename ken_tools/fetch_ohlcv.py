#!/usr/bin/env python3
"""Fetch OHLCV data via ccxt. Reads JSON from stdin, writes JSON to stdout."""

import json
import sys
import os
from pathlib import Path

def fetch_ohlcv(params):
    try:
        import ccxt
    except ImportError:
        return {"error": "ccxt not installed. Run: pip install ccxt"}

    symbol = params["symbol"]
    timeframe = params["timeframe"]
    exchange_id = params.get("exchange", "binance")
    limit = params.get("limit", 1000)

    # Check cache first
    cache_dir = Path("data")
    cache_key = f"{exchange_id}_{symbol.replace('/', '_')}_{timeframe}"
    cache_file = cache_dir / f"{cache_key}.json"

    if cache_file.exists():
        cached = json.loads(cache_file.read_text())
        age_info = f"Cached data ({len(cached['candles'])} candles)"
        return {**cached, "source": "cache", "info": age_info}

    # Fetch from exchange
    exchange_class = getattr(ccxt, exchange_id, None)
    if exchange_class is None:
        return {"error": f"Unknown exchange: {exchange_id}"}

    exchange = exchange_class({"enableRateLimit": True})

    try:
        ohlcv = exchange.fetch_ohlcv(symbol, timeframe, limit=limit)
    except ccxt.BadSymbol:
        # Suggest similar symbols
        try:
            exchange.load_markets()
            suggestions = [s for s in exchange.symbols if symbol.split("/")[0] in s][:5]
            return {"error": f"Symbol {symbol} not found", "suggestions": suggestions}
        except Exception:
            return {"error": f"Symbol {symbol} not found on {exchange_id}"}
    except ccxt.RateLimitExceeded:
        return {"error": "Rate limited. Try again in a few seconds."}
    except Exception as e:
        return {"error": str(e)}

    candles = []
    for row in ohlcv:
        candles.append({
            "timestamp": row[0],
            "open": row[1],
            "high": row[2],
            "low": row[3],
            "close": row[4],
            "volume": row[5],
        })

    # Validate data completeness
    total = len(candles)
    missing = sum(1 for c in candles if c["close"] is None or c["close"] == 0)
    missing_pct = (missing / total * 100) if total > 0 else 0
    warning = None
    if missing_pct > 5:
        warning = f"WARNING: {missing_pct:.1f}% missing candles ({missing}/{total})"

    result = {
        "symbol": symbol,
        "timeframe": timeframe,
        "exchange": exchange_id,
        "candles": candles,
        "total": total,
        "source": "live",
    }
    if warning:
        result["warning"] = warning

    # Cache the result
    cache_dir.mkdir(exist_ok=True)
    cache_file.write_text(json.dumps(result))

    return result


if __name__ == "__main__":
    params = json.load(sys.stdin)
    result = fetch_ohlcv(params)
    json.dump(result, sys.stdout)
