#!/usr/bin/env python3
"""Simple backtester. Reads strategy + OHLCV from stdin JSON, writes results to stdout."""

import json
import sys
from pathlib import Path


def run_backtest(params):
    strategy = params.get("strategy", {})
    symbol = params.get("symbol", "")
    timeframe = params.get("timeframe", "")

    # Load OHLCV data from cache
    cache_dir = Path("data")
    exchange = params.get("exchange", "binance")
    cache_key = f"{exchange}_{symbol.replace('/', '_')}_{timeframe}"
    cache_file = cache_dir / f"{cache_key}.json"

    if not cache_file.exists():
        return {"error": f"No cached data for {symbol} {timeframe}. Run fetch_ohlcv first."}

    data = json.loads(cache_file.read_text())
    candles = data.get("candles", [])

    if len(candles) < 50:
        return {"error": f"Insufficient data: need 50+ candles, have {len(candles)}"}

    # Extract strategy parameters
    sl_logic = strategy.get("sl_logic", {})
    tp_logic = strategy.get("tp_logic", {})
    entry_rules = strategy.get("entry_rules", [])

    # Simple simulation: use entry signals from the strategy definition
    # This is a simplified backtester — real implementation would evaluate
    # EntryLogic against actual indicator values
    trades = simulate_trades(candles, sl_logic, tp_logic)

    if not trades:
        return {
            "trades": [],
            "win_rate": 0.0,
            "avg_rr": 0.0,
            "max_drawdown": 0.0,
            "sharpe": 0.0,
            "total_pnl": 0.0,
            "info": "No trades generated. Check entry rules.",
        }

    # Calculate statistics
    wins = [t for t in trades if t["pnl"] > 0]
    win_rate = len(wins) / len(trades) if trades else 0
    avg_rr = sum(t["pnl_pct"] for t in trades) / len(trades) if trades else 0

    # Max drawdown
    equity = [0.0]
    for t in trades:
        equity.append(equity[-1] + t["pnl_pct"])
    peak = equity[0]
    max_dd = 0.0
    for e in equity:
        if e > peak:
            peak = e
        dd = peak - e
        if dd > max_dd:
            max_dd = dd

    total_pnl = sum(t["pnl_pct"] for t in trades)

    # Sharpe ratio (simplified: mean return / std dev of returns)
    returns = [t["pnl_pct"] for t in trades]
    mean_ret = sum(returns) / len(returns) if returns else 0
    variance = sum((r - mean_ret) ** 2 for r in returns) / len(returns) if returns else 0
    std_dev = variance ** 0.5
    sharpe = (mean_ret / std_dev) if std_dev > 0 else 0

    return {
        "trades": trades,
        "win_rate": round(win_rate, 4),
        "avg_rr": round(avg_rr, 4),
        "max_drawdown": round(max_dd, 4),
        "sharpe": round(sharpe, 4),
        "total_pnl": round(total_pnl, 4),
        "total_trades": len(trades),
    }


def simulate_trades(candles, sl_logic, tp_logic):
    """Simplified trade simulation using SL/TP logic against price action."""
    trades = []
    sl_type = sl_logic.get("type", "Fixed")
    tp_type = tp_logic.get("type", "RiskReward")

    # Default parameters
    sl_distance_pct = 0.02  # 2% default SL
    tp_distance_pct = 0.04  # 4% default TP (2:1 R:R)

    if sl_type == "Fixed":
        sl_distance_pct = sl_logic.get("pips", 200) / 10000
    elif sl_type == "Atr":
        # Use ATR approximation from candle ranges
        atr_mult = sl_logic.get("multiplier", 1.5)
        ranges = [(c["high"] - c["low"]) / c["close"] for c in candles[-14:] if c["close"] > 0]
        avg_range = sum(ranges) / len(ranges) if ranges else 0.02
        sl_distance_pct = avg_range * atr_mult

    if tp_type == "RiskReward":
        ratio = tp_logic.get("ratio", 2.0)
        tp_distance_pct = sl_distance_pct * ratio
    elif tp_type == "Fixed":
        tp_distance_pct = tp_logic.get("pips", 400) / 10000

    # Simulate: enter every N candles (simplified — real impl uses EntryLogic)
    i = 20  # skip first 20 candles for indicator warmup
    while i < len(candles) - 1:
        entry_candle = candles[i]
        entry_price = entry_candle["close"]
        if entry_price <= 0:
            i += 1
            continue

        sl_price = entry_price * (1 - sl_distance_pct)
        tp_price = entry_price * (1 + tp_distance_pct)

        # Walk forward to find exit
        for j in range(i + 1, min(i + 100, len(candles))):
            c = candles[j]
            # Check SL hit
            if c["low"] <= sl_price:
                pnl = sl_price - entry_price
                trades.append({
                    "entry_time": entry_candle["timestamp"],
                    "exit_time": c["timestamp"],
                    "direction": "long",
                    "entry_price": entry_price,
                    "exit_price": sl_price,
                    "pnl": round(pnl, 8),
                    "pnl_pct": round(pnl / entry_price, 6),
                })
                i = j + 5  # cooldown
                break
            # Check TP hit
            if c["high"] >= tp_price:
                pnl = tp_price - entry_price
                trades.append({
                    "entry_time": entry_candle["timestamp"],
                    "exit_time": c["timestamp"],
                    "direction": "long",
                    "entry_price": entry_price,
                    "exit_price": tp_price,
                    "pnl": round(pnl, 8),
                    "pnl_pct": round(pnl / entry_price, 6),
                })
                i = j + 5
                break
        else:
            i += 10  # no exit found, skip ahead

        i += 1

    return trades


if __name__ == "__main__":
    params = json.load(sys.stdin)
    result = run_backtest(params)
    json.dump(result, sys.stdout)
