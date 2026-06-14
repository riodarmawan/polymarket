"""
Full Simulation Demo
====================
Demo lengkap: Data Collection + Analysis + Paper Trading
"""

import asyncio
from datetime import datetime

from bot.config import BotConfig
from bot.api.gamma import GammaAPIClient
from bot.api.clob import CLOBAPIClient
from bot.storage.database import Database
from bot.storage.collector import DataCollector
from bot.analyzers.orderbook import OrderBookAnalyzer
from bot.engine.decision import DecisionEngine
from bot.paper_trading.engine import PaperTradingEngine
from bot.models.probability import Signal


async def run_full_simulation():
    """Jalankan simulasi lengkap."""
    print("=" * 70)
    print("🤖 POLYMARKET PAPER TRADING - FULL SIMULATION")
    print("=" * 70)
    print(f"Waktu mulai: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()
    
    # Inisialisasi
    config = BotConfig(
        initial_capital=1000,
        enable_trading=False,
    )
    
    db = Database()
    collector = DataCollector(db)
    paper_engine = PaperTradingEngine(config, db)
    
    # Step 1: Collect market data
    print("=" * 70)
    print("STEP 1: Collecting Market Data from Gamma API")
    print("=" * 70)
    
    try:
        markets = await collector.collect_markets(limit=20, min_volume=10000)
        print(f"✓ Fetched {len(markets)} markets")
        
        # Tampilkan top 5 markets
        print("\nTop 5 Markets by Volume:")
        for i, m in enumerate(markets[:5], 1):
            print(f"  {i}. {m.question[:60]}")
            print(f"     Yes: ${m.yes_price:.3f} | No: ${m.no_price:.3f} | Volume: ${m.volume:,.0f}")
    except Exception as e:
        print(f"✗ Error fetching markets: {e}")
        print("  Menggunakan mock data...")
        markets = []
    
    # Step 2: Collect order book for first market
    print("\n" + "=" * 70)
    print("STEP 2: Collecting Order Book Data")
    print("=" * 70)
    
    orderbook_data = None
    if markets and markets[0].yes_token_id:
        try:
            orderbook_data = await collector.collect_orderbook(markets[0].yes_token_id)
            if orderbook_data:
                print(f"✓ Order book fetched for: {markets[0].question[:50]}")
                metrics = orderbook_data.get("metrics", {})
                print(f"  Spread: {metrics.get('spread_pct', 0):.2%}")
                print(f"  Bid Depth: {metrics.get('bid_depth', 0):,.0f}")
                print(f"  Ask Depth: {metrics.get('ask_depth', 0):,.0f}")
                print(f"  OBI: {metrics.get('obi', 0):.3f}")
                print(f"  Liquidity Score: {metrics.get('liquidity_score', 0):.2f}")
                print(f"  Tradeable: {metrics.get('is_tradeable', False)}")
        except Exception as e:
            print(f"✗ Error fetching order book: {e}")
    
    # Step 3: Evaluate market
    print("\n" + "=" * 70)
    print("STEP 3: Evaluating Market with Decision Engine")
    print("=" * 70)
    
    if markets:
        market = markets[0]
        
        # Buat mock signals (dalam production, ini dari LLM/news analysis)
        signals = [
            Signal(name="news", value=0.4, confidence=0.7),
            Signal(name="polling", value=0.3, confidence=0.6),
            Signal(name="price_history", value=0.2, confidence=0.7),
            Signal(name="volume", value=0.5, confidence=0.6),
        ]
        
        # Mock order book jika tidak ada
        if not orderbook_data:
            orderbook_bids = [{"price": str(market.yes_price - 0.02), "size": "5000"}]
            orderbook_asks = [{"price": str(market.yes_price + 0.02), "size": "3000"}]
        else:
            orderbook_bids = orderbook_data["data"]["bids"]
            orderbook_asks = orderbook_data["data"]["asks"]
        
        # Evaluate
        decision = paper_engine.evaluate_market(
            market_id=market.id,
            market_question=market.question,
            market_price=market.yes_price,
            orderbook_bids=orderbook_bids,
            orderbook_asks=orderbook_asks,
            signals=signals,
            volume_24h=market.volume,
        )
        
        print(f"\nMarket: {market.question[:60]}")
        print(f"\nDecision: {decision.decision.value.upper()}")
        print(f"Summary: {decision.summary}")
        
        if decision.probability:
            print(f"\nProbability Analysis:")
            print(f"  q_model: {decision.probability.raw_probability:.2%}")
            print(f"  q_conservative: {decision.probability.conservative_probability:.2%}")
            print(f"  uncertainty: {decision.probability.uncertainty:.2%}")
            print(f"  confidence: {decision.probability.confidence:.2%}")
        
        if decision.ev_result:
            print(f"\nExpected Value:")
            print(f"  Edge: {decision.ev_result.edge:.2%}")
            print(f"  EV net: {decision.ev_result.ev_net:.2%}")
            print(f"  EV conservative: {decision.ev_result.ev_conservative:.2%}")
            print(f"  Recommendation: {decision.ev_result.recommendation}")
        
        if decision.position_size:
            print(f"\nPosition Size:")
            print(f"  Kelly fraction: {decision.position_size.kelly_fraction:.2%}")
            print(f"  Position USD: ${decision.position_size.position_usd:.2f}")
            print(f"  Risk per trade: ${decision.position_size.risk_per_trade:.2f}")
        
        # Step 4: Execute paper trade
        print("\n" + "=" * 70)
        print("STEP 4: Executing Paper Trade")
        print("=" * 70)
        
        if decision.decision.value == "entry":
            position = paper_engine.execute_paper_trade(decision)
            if position:
                print(f"✓ Paper trade executed!")
                print(f"  Market: {position.market_question[:50]}")
                print(f"  Side: {position.side.upper()}")
                print(f"  Entry Price: ${position.entry_price:.3f}")
                print(f"  Size: ${position.size_usd:.2f} ({position.size_tokens:.2f} tokens)")
            else:
                print("✗ Trade tidak dapat dieksekusi")
        else:
            print(f"⊘ Trade tidak dieksekusi: {decision.decision.value}")
    
    # Step 5: Portfolio Summary
    print("\n" + "=" * 70)
    print("STEP 5: Portfolio Summary")
    print("=" * 70)
    
    summary = paper_engine.get_portfolio_summary()
    
    portfolio = summary["portfolio"]
    print(f"\nCapital:")
    print(f"  Initial: ${portfolio['initial_capital']:.2f}")
    print(f"  Current: ${portfolio['current_capital']:.2f}")
    print(f"  Total Invested: ${portfolio['total_invested']:.2f}")
    
    print(f"\nPerformance:")
    print(f"  Total Return: {portfolio['total_return']:.2%}")
    print(f"  Total Trades: {portfolio['total_trades']}")
    print(f"  Win Rate: {portfolio['win_rate']:.2%}")
    
    print(f"\nOpen Positions: {summary['open_positions_count']}")
    for pos in summary["open_positions"]:
        print(f"  - {pos['market_question'][:40]} ({pos['side'].upper()})")
        print(f"    Entry: ${pos['entry_price']:.3f} | Current: ${pos['current_price']:.3f}")
    
    # Step 6: Database Stats
    print("\n" + "=" * 70)
    print("STEP 6: Database Statistics")
    print("=" * 70)
    
    stats = db.get_stats()
    print(f"\nDatabase Stats:")
    print(f"  Total Markets: {stats.get('total_markets', 0)}")
    print(f"  Active Markets: {stats.get('active_markets', 0)}")
    print(f"  Order Book Snapshots: {stats.get('orderbook_snapshots', 0)}")
    print(f"  Price Records: {stats.get('price_records', 0)}")
    print(f"  Paper Positions: {stats.get('total_positions', 0)}")
    print(f"  Decisions Made: {stats.get('total_decisions', 0)}")
    
    print("\n" + "=" * 70)
    print("✓ SIMULASI SELESAI!")
    print("=" * 70)
    print(f"Data tersimpan di: {db.db_path}")
    print()
    
    return summary


def main():
    """Jalankan simulasi."""
    asyncio.run(run_full_simulation())


if __name__ == "__main__":
    main()
