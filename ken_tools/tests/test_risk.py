"""Tests for risk analysis."""

import json
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from risk_analysis import compute_risk_metrics


def test_empty_trades():
    result = compute_risk_metrics({"trades": []})
    assert result["win_rate"] == 0.0
    assert result["sharpe_ratio"] == 0.0
    assert result["max_drawdown"] == 0.0


def test_all_winning_trades():
    trades = [
        {"pnl": 10, "pnl_pct": 0.02},
        {"pnl": 15, "pnl_pct": 0.03},
        {"pnl": 5, "pnl_pct": 0.01},
    ]
    result = compute_risk_metrics({"trades": trades})
    assert result["win_rate"] == 1.0
    assert result["max_drawdown"] == 0.0
    assert result["wins"] == 3
    assert result["losses"] == 0


def test_mixed_trades():
    trades = [
        {"pnl": 10, "pnl_pct": 0.02},
        {"pnl": -5, "pnl_pct": -0.01},
        {"pnl": 8, "pnl_pct": 0.016},
        {"pnl": -3, "pnl_pct": -0.006},
    ]
    result = compute_risk_metrics({"trades": trades})
    assert 0.0 < result["win_rate"] < 1.0
    assert result["wins"] == 2
    assert result["losses"] == 2
    assert result["max_drawdown"] >= 0


def test_single_trade():
    trades = [{"pnl": 10, "pnl_pct": 0.02}]
    result = compute_risk_metrics({"trades": trades})
    assert result["win_rate"] == 1.0
    assert result["total_trades"] == 1
