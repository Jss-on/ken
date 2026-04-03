"""Tests for the backtester."""

import json
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from backtest_strategy import simulate_trades, run_backtest


def make_candles(n=100, start_price=100.0):
    """Generate synthetic OHLCV candles for testing."""
    import random
    random.seed(42)
    candles = []
    price = start_price
    for i in range(n):
        change = random.uniform(-0.02, 0.02) * price
        o = price
        c = price + change
        h = max(o, c) + abs(change) * 0.5
        l = min(o, c) - abs(change) * 0.5
        candles.append({
            "timestamp": 1700000000000 + i * 3600000,
            "open": round(o, 2),
            "high": round(h, 2),
            "low": round(l, 2),
            "close": round(c, 2),
            "volume": round(random.uniform(100, 1000), 2),
        })
        price = c
    return candles


def test_simulate_trades_produces_results():
    candles = make_candles(200)
    sl = {"type": "Fixed", "pips": 200}
    tp = {"type": "RiskReward", "ratio": 2.0}
    trades = simulate_trades(candles, sl, tp)
    assert len(trades) > 0
    for t in trades:
        assert "entry_price" in t
        assert "exit_price" in t
        assert "pnl" in t
        assert "pnl_pct" in t


def test_simulate_trades_zero_candles():
    trades = simulate_trades([], {"type": "Fixed", "pips": 200}, {"type": "Fixed", "pips": 400})
    assert trades == []


def test_simulate_trades_few_candles():
    candles = make_candles(10)
    trades = simulate_trades(candles, {"type": "Fixed", "pips": 200}, {"type": "Fixed", "pips": 400})
    assert trades == []  # Not enough candles (need 20+ for warmup)


def test_backtest_result_stats():
    candles = make_candles(500)
    sl = {"type": "Atr", "multiplier": 1.5}
    tp = {"type": "RiskReward", "ratio": 2.0}
    trades = simulate_trades(candles, sl, tp)
    if trades:
        wins = [t for t in trades if t["pnl"] > 0]
        win_rate = len(wins) / len(trades)
        assert 0.0 <= win_rate <= 1.0
