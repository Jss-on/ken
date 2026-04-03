#!/usr/bin/env python3
"""Compute risk metrics from backtest results. Reads JSON stdin, writes JSON stdout."""

import json
import sys
import math


def compute_risk_metrics(params):
    result = params.get("backtest_result", params)
    trades = result.get("trades", [])

    if not trades:
        return {
            "sharpe_ratio": 0.0,
            "sortino_ratio": 0.0,
            "max_drawdown": 0.0,
            "calmar_ratio": 0.0,
            "win_rate": 0.0,
            "avg_rr": 0.0,
        }

    returns = [t["pnl_pct"] for t in trades]
    wins = [r for r in returns if r > 0]
    losses = [r for r in returns if r <= 0]

    win_rate = len(wins) / len(returns) if returns else 0
    avg_win = sum(wins) / len(wins) if wins else 0
    avg_loss = abs(sum(losses) / len(losses)) if losses else 0
    avg_rr = (avg_win / avg_loss) if avg_loss > 0 else float("inf")

    mean_ret = sum(returns) / len(returns)
    variance = sum((r - mean_ret) ** 2 for r in returns) / len(returns)
    std_dev = math.sqrt(variance) if variance > 0 else 0

    # Sharpe ratio (annualized, assuming ~252 trading days)
    sharpe = (mean_ret / std_dev * math.sqrt(252)) if std_dev > 0 else 0

    # Sortino ratio (downside deviation only)
    downside = [min(r - mean_ret, 0) ** 2 for r in returns]
    downside_dev = math.sqrt(sum(downside) / len(downside)) if downside else 0
    sortino = (mean_ret / downside_dev * math.sqrt(252)) if downside_dev > 0 else 0

    # Max drawdown
    equity = [0.0]
    for r in returns:
        equity.append(equity[-1] + r)
    peak = equity[0]
    max_dd = 0.0
    for e in equity:
        if e > peak:
            peak = e
        dd = peak - e
        if dd > max_dd:
            max_dd = dd

    # Calmar ratio (annualized return / max drawdown)
    annual_return = mean_ret * 252
    calmar = (annual_return / max_dd) if max_dd > 0 else 0

    return {
        "sharpe_ratio": round(sharpe, 4),
        "sortino_ratio": round(sortino, 4),
        "max_drawdown": round(max_dd, 4),
        "calmar_ratio": round(calmar, 4),
        "win_rate": round(win_rate, 4),
        "avg_rr": round(avg_rr, 4),
        "total_trades": len(trades),
        "wins": len(wins),
        "losses": len(losses),
    }


if __name__ == "__main__":
    params = json.load(sys.stdin)
    result = compute_risk_metrics(params)
    json.dump(result, sys.stdout)
