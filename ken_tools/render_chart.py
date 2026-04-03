#!/usr/bin/env python3
"""Render interactive chart using Lightweight Charts. Opens HTML in browser."""

import json
import sys
import tempfile
import webbrowser
from pathlib import Path


def render_chart(params):
    symbol = params.get("symbol", "")
    timeframe = params.get("timeframe", "")
    signals = params.get("signals", [])
    sl_tp = params.get("sl_tp", {})

    # Load cached OHLCV data
    cache_dir = Path("data")
    exchange = params.get("exchange", "binance")
    cache_key = f"{exchange}_{symbol.replace('/', '_')}_{timeframe}"
    cache_file = cache_dir / f"{cache_key}.json"

    if not cache_file.exists():
        return {"error": f"No data for {symbol} {timeframe}. Run fetch_ohlcv first."}

    data = json.loads(cache_file.read_text())
    candles = data.get("candles", [])

    # Convert timestamps to seconds for Lightweight Charts
    chart_data = []
    for c in candles:
        chart_data.append({
            "time": c["timestamp"] // 1000,  # ms to seconds
            "open": c["open"],
            "high": c["high"],
            "low": c["low"],
            "close": c["close"],
        })

    # Build signal markers
    markers = []
    for s in signals:
        color = "#26a69a" if s.get("signal_type") == "bullish" else "#ef5350"
        shape = "arrowUp" if s.get("signal_type") == "bullish" else "arrowDown"
        position = "belowBar" if s.get("signal_type") == "bullish" else "aboveBar"
        markers.append({
            "time": s["timestamp"] // 1000,
            "position": position,
            "color": color,
            "shape": shape,
            "text": f"{s.get('indicator_name', '')} ({s.get('strength', 0):.2f})",
        })

    html = generate_html(symbol, timeframe, chart_data, markers, sl_tp)

    # Write to temp file and open
    tmp = tempfile.NamedTemporaryFile(suffix=".html", delete=False, mode="w")
    tmp.write(html)
    tmp.close()

    webbrowser.open(f"file://{tmp.name}")

    return {
        "status": "opened",
        "file": tmp.name,
        "candles": len(chart_data),
        "markers": len(markers),
    }


def generate_html(symbol, timeframe, candles, markers, sl_tp):
    candles_json = json.dumps(candles)
    markers_json = json.dumps(markers)

    sl_lines = ""
    if sl_tp.get("sl"):
        sl_lines += f"""
        candleSeries.createPriceLine({{
            price: {sl_tp['sl']},
            color: '#ef5350',
            lineWidth: 2,
            lineStyle: 2,
            axisLabelVisible: true,
            title: 'SL',
        }});"""
    if sl_tp.get("tp"):
        sl_lines += f"""
        candleSeries.createPriceLine({{
            price: {sl_tp['tp']},
            color: '#26a69a',
            lineWidth: 2,
            lineStyle: 2,
            axisLabelVisible: true,
            title: 'TP',
        }});"""

    return f"""<!DOCTYPE html>
<html>
<head>
    <title>Ken — {symbol} {timeframe}</title>
    <script src="https://unpkg.com/lightweight-charts/dist/lightweight-charts.standalone.production.js"></script>
    <style>
        body {{ margin: 0; background: #1a1a2e; font-family: system-ui; }}
        #chart {{ width: 100vw; height: 100vh; }}
        .header {{ position: absolute; top: 10px; left: 10px; z-index: 10;
                   color: #e0e0e0; font-size: 14px; }}
        .header h3 {{ margin: 0; color: #fff; }}
    </style>
</head>
<body>
    <div class="header">
        <h3>Ken — {symbol} {timeframe}</h3>
    </div>
    <div id="chart"></div>
    <script>
        const chart = LightweightCharts.createChart(document.getElementById('chart'), {{
            width: window.innerWidth,
            height: window.innerHeight,
            layout: {{
                background: {{ type: 'solid', color: '#1a1a2e' }},
                textColor: '#e0e0e0',
            }},
            grid: {{
                vertLines: {{ color: '#2a2a4a' }},
                horzLines: {{ color: '#2a2a4a' }},
            }},
            crosshair: {{ mode: LightweightCharts.CrosshairMode.Normal }},
            timeScale: {{ timeVisible: true }},
        }});

        const candleSeries = chart.addCandlestickSeries({{
            upColor: '#26a69a',
            downColor: '#ef5350',
            borderDownColor: '#ef5350',
            borderUpColor: '#26a69a',
            wickDownColor: '#ef5350',
            wickUpColor: '#26a69a',
        }});

        candleSeries.setData({candles_json});

        const markers = {markers_json};
        if (markers.length > 0) {{
            candleSeries.setMarkers(markers);
        }}

        {sl_lines}

        chart.timeScale().fitContent();
        window.addEventListener('resize', () => {{
            chart.applyOptions({{ width: window.innerWidth, height: window.innerHeight }});
        }});
    </script>
</body>
</html>"""


if __name__ == "__main__":
    params = json.load(sys.stdin)
    result = render_chart(params)
    json.dump(result, sys.stdout)
