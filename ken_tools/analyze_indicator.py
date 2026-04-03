#!/usr/bin/env python3
"""Analyze indicator signals on OHLCV data. Reads JSON from stdin, writes signals to stdout."""

import json
import sys
from pathlib import Path


def analyze_indicator(params):
    indicator = params.get("indicator", "").upper()
    symbol = params.get("symbol", "")
    timeframe = params.get("timeframe", "")
    ind_params = params.get("params", {})

    # Load cached OHLCV data
    cache_dir = Path("data")
    exchange = params.get("exchange", "binance")
    cache_key = f"{exchange}_{symbol.replace('/', '_')}_{timeframe}"
    cache_file = cache_dir / f"{cache_key}.json"

    if not cache_file.exists():
        return {"error": f"No data for {symbol} {timeframe}. Run fetch_ohlcv first."}

    data = json.loads(cache_file.read_text())
    candles = data.get("candles", [])
    closes = [c["close"] for c in candles if c["close"] and c["close"] > 0]

    if len(closes) < 20:
        return {"error": f"Need 20+ candles, have {len(closes)}"}

    signals = []

    if indicator == "RSI":
        period = ind_params.get("period", 14)
        overbought = ind_params.get("overbought", 70)
        oversold = ind_params.get("oversold", 30)
        rsi_values = compute_rsi(closes, period)

        for i, (rsi, candle) in enumerate(zip(rsi_values, candles[period:])):
            if rsi <= oversold:
                signals.append({
                    "timestamp": candle["timestamp"],
                    "indicator_name": "RSI",
                    "signal_type": "bullish",
                    "strength": round(1.0 - (rsi / oversold), 4),
                })
            elif rsi >= overbought:
                signals.append({
                    "timestamp": candle["timestamp"],
                    "indicator_name": "RSI",
                    "signal_type": "bearish",
                    "strength": round((rsi - overbought) / (100 - overbought), 4),
                })

    elif indicator == "MACD":
        fast = ind_params.get("fast", 12)
        slow = ind_params.get("slow", 26)
        signal_period = ind_params.get("signal", 9)
        macd_line, signal_line = compute_macd(closes, fast, slow, signal_period)

        for i in range(1, len(macd_line)):
            idx = len(candles) - len(macd_line) + i
            if idx < 0 or idx >= len(candles):
                continue
            # Crossover detection
            if macd_line[i] > signal_line[i] and macd_line[i - 1] <= signal_line[i - 1]:
                signals.append({
                    "timestamp": candles[idx]["timestamp"],
                    "indicator_name": "MACD",
                    "signal_type": "bullish",
                    "strength": round(min(abs(macd_line[i] - signal_line[i]) * 100, 1.0), 4),
                })
            elif macd_line[i] < signal_line[i] and macd_line[i - 1] >= signal_line[i - 1]:
                signals.append({
                    "timestamp": candles[idx]["timestamp"],
                    "indicator_name": "MACD",
                    "signal_type": "bearish",
                    "strength": round(min(abs(macd_line[i] - signal_line[i]) * 100, 1.0), 4),
                })
    else:
        return {
            "error": f"Unknown built-in indicator: {indicator}",
            "available": ["RSI", "MACD"],
            "hint": "Use register_indicator to add custom indicators",
        }

    return {
        "indicator": indicator,
        "symbol": symbol,
        "timeframe": timeframe,
        "total_signals": len(signals),
        "signals": signals[-50:],  # Return last 50 signals
    }


def compute_rsi(closes, period=14):
    """Compute RSI values."""
    deltas = [closes[i] - closes[i - 1] for i in range(1, len(closes))]
    gains = [max(d, 0) for d in deltas]
    losses = [-min(d, 0) for d in deltas]

    avg_gain = sum(gains[:period]) / period
    avg_loss = sum(losses[:period]) / period

    rsi_values = []
    for i in range(period, len(deltas)):
        avg_gain = (avg_gain * (period - 1) + gains[i]) / period
        avg_loss = (avg_loss * (period - 1) + losses[i]) / period
        if avg_loss == 0:
            rsi_values.append(100.0)
        else:
            rs = avg_gain / avg_loss
            rsi_values.append(100 - (100 / (1 + rs)))

    return rsi_values


def compute_macd(closes, fast=12, slow=26, signal=9):
    """Compute MACD line and signal line."""
    def ema(data, period):
        result = [sum(data[:period]) / period]
        multiplier = 2 / (period + 1)
        for price in data[period:]:
            result.append((price - result[-1]) * multiplier + result[-1])
        return result

    ema_fast = ema(closes, fast)
    ema_slow = ema(closes, slow)

    # Align lengths
    diff = len(ema_fast) - len(ema_slow)
    ema_fast = ema_fast[diff:]

    macd_line = [f - s for f, s in zip(ema_fast, ema_slow)]
    signal_line = ema(macd_line, signal) if len(macd_line) >= signal else macd_line

    # Align lengths again
    diff2 = len(macd_line) - len(signal_line)
    macd_line = macd_line[diff2:]

    return macd_line, signal_line


if __name__ == "__main__":
    params = json.load(sys.stdin)
    result = analyze_indicator(params)
    json.dump(result, sys.stdout)
