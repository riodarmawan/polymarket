"""
Demo: Polymarket Trading Bot
============================
Contoh lengkap penggunaan seluruh sistem.
"""

from bot import (
    DecisionEngine,
    BotConfig,
    Signal,
    OrderBookSnapshot,
    OrderBookLevel,
)


def create_demo_signals(
    news_sentiment: float = 0.5,
    news_confidence: float = 0.7,
    polling_sentiment: float = 0.3,
    polling_confidence: float = 0.6,
    price_momentum: float = 0.2,
    volume_signal: float = 0.4,
) -> list:
    """Buat demo signals."""
    return [
        Signal(name="news", value=news_sentiment, confidence=news_confidence),
        Signal(name="polling", value=polling_sentiment, confidence=polling_confidence),
        Signal(name="price_history", value=price_momentum, confidence=0.7),
        Signal(name="volume", value=volume_signal, confidence=0.6),
    ]


def create_demo_orderbook(
    bid_price: float = 0.48,
    ask_price: float = 0.52,
    bid_depth: float = 5000,
    ask_depth: float = 3000,
) -> OrderBookSnapshot:
    """Buat demo order book."""
    return OrderBookSnapshot(
        bids=[OrderBookLevel(price=bid_price, size=bid_depth)],
        asks=[OrderBookLevel(price=ask_price, size=ask_depth)],
    )


def demo_entry_decision():
    """Demo: Keputusan entry."""
    print("=" * 60)
    print("DEMO 1: Keputusan Entry")
    print("=" * 60)
    
    # Konfigurasi
    config = BotConfig(
        initial_capital=1000,
        enable_trading=False,  # Simulasi saja
    )
    
    # Inisialisasi engine
    engine = DecisionEngine(config)
    
    # Contoh market
    market_id = "540817"
    market_question = "New Rihanna Album before GTA VI?"
    market_price = 0.51
    
    # Signals dari analisis
    signals = create_demo_signals(
        news_sentiment=0.6,      # Berita positif
        news_confidence=0.7,
        polling_sentiment=0.4,   # Polling agak positif
        polling_confidence=0.6,
        price_momentum=0.3,      # Harga naik
        volume_signal=0.5,       # Volume cukup
    )
    
    # Order book
    orderbook = create_demo_orderbook(
        bid_price=0.48,
        ask_price=0.52,
        bid_depth=5000,
        ask_depth=3000,
    )
    
    # Evaluasi
    decision = engine.evaluate(
        market_id=market_id,
        market_question=market_question,
        market_price=market_price,
        orderbook=orderbook,
        signals=signals,
        volume_24h=50000,
        capital=1000,
        side="yes",
    )
    
    # Output
    print(f"\nMarket: {market_question}")
    print(f"Harga: {market_price}")
    print(f"\nKeputusan: {decision.decision.value.upper()}")
    print(f"\nSummary: {decision.summary}")
    
    print("\n--- Probabilitas ---")
    if decision.probability:
        p = decision.probability
        print(f"  q_model: {p.raw_probability:.2%}")
        print(f"  q_conservative: {p.conservative_probability:.2%}")
        print(f"  uncertainty: {p.uncertainty:.2%}")
        print(f"  confidence: {p.confidence:.2%}")
    
    print("\n--- Order Book ---")
    if decision.orderbook_metrics:
        ob = decision.orderbook_metrics
        print(f"  Best bid: {ob.best_bid}")
        print(f"  Best ask: {ob.best_ask}")
        print(f"  Spread: {ob.spread:.4f} ({ob.spread_pct:.2%})")
        print(f"  Liquidity score: {ob.liquidity_score:.2f}")
        print(f"  Market phase: {ob.market_phase.value}")
        print(f"  Tradeable: {ob.is_tradeable}")
    
    print("\n--- Expected Value ---")
    if decision.ev_result:
        ev = decision.ev_result
        print(f"  Edge: {ev.edge:.2%}")
        print(f"  EV net: {ev.ev_net:.2%}")
        print(f"  EV conservative: {ev.ev_conservative:.2%}")
        print(f"  Recommendation: {ev.recommendation}")
    
    print("\n--- Position Size ---")
    if decision.position_size:
        ps = decision.position_size
        print(f"  Kelly fraction: {ps.kelly_fraction:.2%}")
        print(f"  Adjusted fraction: {ps.adjusted_fraction:.2%}")
        print(f"  Position USD: ${ps.position_usd:.2f}")
        print(f"  Position tokens: {ps.position_tokens:.2f}")
        print(f"  Risk per trade: ${ps.risk_per_trade:.2f}")
        print(f"  Valid: {ps.is_valid}")
    
    if decision.reasons:
        print("\n--- Alasan ---")
        for r in decision.reasons:
            print(f"  • {r}")
    
    if decision.warnings:
        print("\n--- Warning ---")
        for w in decision.warnings:
            print(f"  ⚠ {w}")
    
    return decision


def demo_skip_decision():
    """Demo: Keputusan skip (spread terlalu lebar)."""
    print("\n" + "=" * 60)
    print("DEMO 2: Skip Decision (Spread Terlalu Lebar)")
    print("=" * 60)
    
    config = BotConfig(initial_capital=1000)
    engine = DecisionEngine(config)
    
    # Order book dengan spread lebar
    orderbook = create_demo_orderbook(
        bid_price=0.40,
        ask_price=0.60,  # Spread 20%!
        bid_depth=1000,
        ask_depth=500,
    )
    
    signals = create_demo_signals(
        news_sentiment=0.5,
        news_confidence=0.7,
    )
    
    decision = engine.evaluate(
        market_id="999999",
        market_question="Test Market with Wide Spread",
        market_price=0.50,
        orderbook=orderbook,
        signals=signals,
        volume_24h=5000,
    )
    
    print(f"\nKeputusan: {decision.decision.value.upper()}")
    print(f"Summary: {decision.summary}")
    
    return decision


def demo_exit_decision():
    """Demo: Keputusan exit."""
    print("\n" + "=" * 60)
    print("DEMO 3: Exit Decision")
    print("=" * 60)
    
    config = BotConfig()
    engine = DecisionEngine(config)
    
    # Simulasi posisi yang sudah profit
    exit_result = engine.evaluate_exit(
        market_id="540817",
        q_model=0.70,      # Probabilitas naik
        p_current=0.65,    # Harga naik dari 0.51
        p_entry=0.51,      # Harga entry
        side="yes",
    )
    
    print(f"\nHarga entry: 0.51")
    print(f"Harga sekarang: 0.65")
    print(f"Probabilitas model: 70%")
    print(f"\nUnrealized P&L: {exit_result['unrealized_pnl']:.2%}")
    print(f"Current edge: {exit_result['current_edge']:.2%}")
    print(f"Should exit: {exit_result['should_exit']}")
    print(f"Reason: {exit_result['reason'] or 'Tidak ada'}")
    
    return exit_result


def main():
    """Jalankan semua demo."""
    print("\n🤖 POLYMARKET TRADING BOT - DEMO\n")
    
    # Demo 1: Entry decision
    demo_entry_decision()
    
    # Demo 2: Skip decision
    demo_skip_decision()
    
    # Demo 3: Exit decision
    demo_exit_decision()
    
    print("\n" + "=" * 60)
    print("SELESAI!")
    print("=" * 60)


if __name__ == "__main__":
    main()
