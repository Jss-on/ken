"""Tests for indicator analysis."""

import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from analyze_indicator import compute_rsi, compute_macd


def test_rsi_bounds():
    """RSI values should always be between 0 and 100."""
    closes = [100 + i * 0.5 for i in range(50)]  # Trending up
    rsi = compute_rsi(closes, 14)
    for val in rsi:
        assert 0.0 <= val <= 100.0, f"RSI {val} out of bounds"


def test_rsi_trending_up():
    """RSI should be high when prices trend up consistently."""
    closes = [100 + i * 2.0 for i in range(50)]
    rsi = compute_rsi(closes, 14)
    assert rsi[-1] > 70, f"Expected RSI > 70 for uptrend, got {rsi[-1]}"


def test_rsi_trending_down():
    """RSI should be low when prices trend down consistently."""
    closes = [100 - i * 2.0 for i in range(50)]
    rsi = compute_rsi(closes, 14)
    assert rsi[-1] < 30, f"Expected RSI < 30 for downtrend, got {rsi[-1]}"


def test_macd_produces_output():
    closes = [100 + i * 0.1 for i in range(50)]
    macd_line, signal_line = compute_macd(closes, 12, 26, 9)
    assert len(macd_line) > 0
    assert len(signal_line) > 0
    assert len(macd_line) == len(signal_line)


def test_macd_crossover_detection():
    """MACD line and signal line should diverge during a trend."""
    import math
    # Sinusoidal price action produces clear MACD oscillation
    closes = [100 + 10 * math.sin(i * 0.1) for i in range(200)]
    macd_line, signal_line = compute_macd(closes, 12, 26, 9)
    # In oscillating data, MACD should cross signal multiple times
    crossovers = 0
    for i in range(1, len(macd_line)):
        if (macd_line[i] > signal_line[i]) != (macd_line[i-1] > signal_line[i-1]):
            crossovers += 1
    assert crossovers >= 1, f"Expected crossovers in oscillating data, got {crossovers}"
