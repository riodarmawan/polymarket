"""
Complete Simulation Demo
========================
Demo lengkap dengan mock data untuk testing.
"""

import asyncio
import json
from datetime import datetime
from typing import Dict, List

from bot.config import BotConfig
from bot.storage.database import Database
from bot.analyzers.orderbook import OrderBookAnalyzer, OrderBookSnapshot, OrderBookLevel
from bot.engine.decision import DecisionEngine, Decision
from bot.paper_trading.engine import PaperTradingEngine
from bot.models.probability import Signal
from bot.utils.mock_data import MockDataProvider


class SimulationRunner:
    """
    Runner untuk simulasi lengkap.
    
    Alur:
    1. Load mock market data
    2. Analyze order book
    3. Generate signals (simulasi news/LLM)
    4. Evaluate with decision engine
    5. Execute paper trade
    6. Update prices (simulasi waktu berjalan)
    7. Track P&L
    """
    
    def __init__(self, initial_capital: float = 1000):
        self.config = BotConfig(
            initial_capital=initial_capital,
            enable_trading=False,
        )
        self.db = Database()
        self.decision_engine = DecisionEngine(self.config)
        self.paper_engine = PaperTradingEngine(self.config, self.db)
        self.orderbook_analyzer = OrderBookAnalyzer()
        
        # Load markets
        self.markets = MockDataProvider.get_mock_markets(limit=5)
        
        # Store price history
        self.price_history: Dict[str, List[float]] = {}
    
    def run_full_cycle(self):
        """Jalankan satu siklus simulasi lengkap."""
        print("\n" + "=" * 70)
        print("🤖 POLYMARKET PAPER TRADING SIMULATION")
        print("=" * 70)
        print(f"Modal: ${self.config.initial_capital:,.2f}")
        print(f"Waktu: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
        print()
        
        # Simpan markets ke database
        for market in self.markets:
            self.db.save_market(market)
        
        # Tampilkan markets
        self._print_markets()
        
        # Evaluasi setiap market
        print("\n" + "=" * 70)
        print("EVALUATING MARKETS")
        print("=" * 70)
        
        for market in self.markets:
            self._evaluate_market(market)
        
        # Simulasi pergerakan harga
        print("\n" + "=" * 70)
        print("SIMULATING PRICE MOVEMENTS (5 iterations)")
        print("=" * 70)
        
        for i in range(5):
            print(f"\n--- Iteration {i+1} ---")
            self._simulate_price_movement()
            self._check_exits()
        
        # Final summary
        self._print_final_summary()
    
    def _print_markets(self):
        """Tampilkan daftar market."""
        print("\n" + "=" * 70)
        print("AVAILABLE MARKETS")
        print("=" * 70)
        
        for i, market in enumerate(self.markets, 1):
            outcome_prices = json.loads(market.get("outcomePrices", "[]"))
            yes_price = float(outcome_prices[0]) if outcome_prices else 0
            no_price = float(outcome_prices[1]) if len(outcome_prices) > 1 else 0
            volume = float(market.get("volume", 0))
            
            print(f"\n{i}. {market['question']}")
            print(f"   Yes: ${yes_price:.3f} | No: ${no_price:.3f}")
            print(f"   Volume: ${volume:,.0f}")
            print(f"   End: {market.get('endDate', 'N/A')}")
    
    def _evaluate_market(self, market: Dict):
        """Evaluasi satu market."""
        print(f"\n--- Evaluating: {market['question'][:50]}... ---")
        
        # Parse data
        outcome_prices = json.loads(market.get("outcomePrices", "[]"))
        yes_price = float(outcome_prices[0]) if outcome_prices else 0.5
        
        # Generate mock order book
        orderbook_data = MockDataProvider.get_mock_orderbook(
            yes_price=yes_price,
            spread=0.04
        )
        
        # Generate mock signals
        signals_data = MockDataProvider.get_mock_signals(market["id"])
        signals = [
            Signal(name=s["name"], value=s["value"], confidence=s["confidence"])
            for s in signals_data
        ]
        
        # Create order book snapshot
        bids = [OrderBookLevel(price=float(b["price"]), size=float(b["size"])) 
                for b in orderbook_data["bids"]]
        asks = [OrderBookLevel(price=float(a["price"]), size=float(a["size"])) 
                for a in orderbook_data["asks"]]
        orderbook = OrderBookSnapshot(bids=bids, asks=asks)
        
        # Analyze order book
        ob_metrics = self.orderbook_analyzer.analyze(orderbook)
        
        print(f"  Order Book Analysis:")
        print(f"    Spread: {ob_metrics.spread_pct:.2%}")
        print(f"    Bid Depth: {ob_metrics.bid_depth:,.0f}")
        print(f"    Ask Depth: {ob_metrics.ask_depth:,.0f}")
        print(f"    OBI: {ob_metrics.order_book_imbalance:.3f}")
        print(f"    Liquidity Score: {ob_metrics.liquidity_score:.2f}")
        print(f"    Tradeable: {ob_metrics.is_tradeable}")
        
        # Evaluate with decision engine
        existing_exposure = sum(
            p.size_usd for p in self.paper_engine.positions if p.status == "open"
        )
        
        decision = self.decision_engine.evaluate(
            market_id=market["id"],
            market_question=market["question"],
            market_price=yes_price,
            orderbook=orderbook,
            signals=signals,
            volume_24h=float(market.get("volume", 0)) / 30,  # Approx daily
            capital=self.paper_engine.portfolio.current_capital,
            existing_exposure=existing_exposure,
        )
        
        print(f"\n  Decision: {decision.decision.value.upper()}")
        print(f"  Summary: {decision.summary}")
        
        if decision.probability:
            print(f"  q_model: {decision.probability.raw_probability:.2%}")
            print(f"  q_conservative: {decision.probability.conservative_probability:.2%}")
        
        if decision.ev_result:
            print(f"  EV net: {decision.ev_result.ev_net:.2%}")
        
        # Execute paper trade if ENTRY
        if decision.decision == Decision.ENTRY and decision.position_size:
            position = self.paper_engine.execute_paper_trade(decision)
            if position:
                print(f"\n  ✓ PAPER TRADE EXECUTED")
                print(f"    Side: {position.side.upper()}")
                print(f"    Entry: ${position.entry_price:.3f}")
                print(f"    Size: ${position.size_usd:.2f}")
                
                # Init price history
                if position.market_id not in self.price_history:
                    self.price_history[position.market_id] = []
                self.price_history[position.market_id].append(position.entry_price)
    
    def _simulate_price_movement(self):
        """Simulasi pergerakan harga untuk semua posisi."""
        for pos in list(self.paper_engine.positions):
            if pos.status == "open":
                # Get current price history
                history = self.price_history.get(pos.market_id, [pos.entry_price])
                current_price = history[-1]
                
                # Simulate movement
                new_price = MockDataProvider.simulate_market_movement(current_price)
                
                # Update history
                self.price_history[pos.market_id].append(new_price)
                
                # Update position
                self.paper_engine.update_position_price(pos.id, new_price)
                
                pnl_pct = pos.unrealized_pnl_pct
                print(f"  {pos.market_question[:40]}...")
                print(f"    Price: ${pos.entry_price:.3f} → ${new_price:.3f} ({pnl_pct:+.2%})")
    
    def _check_exits(self):
        """Cek dan eksekusi stop loss / take profit."""
        for pos in list(self.paper_engine.positions):
            if pos.status == "open":
                # Check stop loss (20%)
                if pos.unrealized_pnl_pct < -0.20:
                    print(f"\n  ⚠ STOP LOSS: {pos.market_question[:40]}...")
                    self.paper_engine.close_position(pos.id)
                
                # Check take profit (30%)
                elif pos.unrealized_pnl_pct > 0.30:
                    print(f"\n  ✓ TAKE PROFIT: {pos.market_question[:40]}...")
                    self.paper_engine.close_position(pos.id)
    
    def _print_final_summary(self):
        """Tampilkan ringkasan akhir."""
        print("\n" + "=" * 70)
        print("FINAL PORTFOLIO SUMMARY")
        print("=" * 70)
        
        summary = self.paper_engine.get_portfolio_summary()
        portfolio = summary["portfolio"]
        
        print(f"\nCapital:")
        print(f"  Initial:  ${portfolio['initial_capital']:,.2f}")
        print(f"  Current:  ${portfolio['current_capital']:,.2f}")
        print(f"  P&L:      ${portfolio['current_capital'] - portfolio['initial_capital']:,.2f}")
        print(f"  Return:   {portfolio['total_return']:.2%}")
        
        print(f"\nTrading Stats:")
        print(f"  Total Trades: {portfolio['total_trades']}")
        print(f"  Winning:      {portfolio['winning_trades']}")
        print(f"  Losing:       {portfolio['losing_trades']}")
        print(f"  Win Rate:     {portfolio['win_rate']:.2%}")
        
        print(f"\nOpen Positions: {summary['open_positions_count']}")
        for pos in summary["open_positions"]:
            print(f"  - {pos['market_question'][:40]}...")
            print(f"    {pos['side'].upper()} @ ${pos['entry_price']:.3f}")
            print(f"    Current: ${pos['current_price']:.3f}")
        
        # Database stats
        stats = self.db.get_stats()
        print(f"\nDatabase:")
        print(f"  Markets: {stats.get('total_markets', 0)}")
        print(f"  Decisions: {stats.get('total_decisions', 0)}")
        print(f"  Positions: {stats.get('total_positions', 0)}")
        
        print("\n" + "=" * 70)
        print("✓ SIMULASI SELESAI!")
        print("=" * 70)


def main():
    """Jalankan simulasi."""
    runner = SimulationRunner(initial_capital=1000)
    runner.run_full_cycle()


if __name__ == "__main__":
    main()
