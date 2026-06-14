"""
Probability Model - Bayesian Updating
=====================================
Model probabilitas untuk mengestimasi q (probabilitas sebenarnya)
menggunakan Bayesian updating dengan multiple signals.
"""

import math
from dataclasses import dataclass, field
from typing import Dict, List, Optional
from datetime import datetime


@dataclass
class Signal:
    """Sinyal input untuk model probabilitas."""
    name: str
    value: float          # Nilai sinyal (-1 sampai 1)
    confidence: float     # Confidence sinyal (0 sampai 1)
    weight: float = 1.0   # Bobot sinyal
    timestamp: Optional[datetime] = None
    
    def __post_init__(self):
        if self.timestamp is None:
            self.timestamp = datetime.now()


@dataclass
class ProbabilityEstimate:
    """Hasil estimasi probabilitas."""
    raw_probability: float        # q_model mentah
    conservative_probability: float  # q_conservative (dikurangi uncertainty)
    uncertainty: float            # Tingkat ketidakpastian
    confidence: float             # Confidence model
    signals_used: List[str]      # Sinyal yang digunakan
    logit_value: float = 0.0     # Nilai logit (untuk debugging)
    
    @property
    def edge_vs_price(self):
        """Edge vs harga (harus diisi dari luar)."""
        return None


class BayesianProbabilityModel:
    """
    Model probabilitas menggunakan Bayesian updating.
    
    Prinsip:
    - Mulai dari prior probability
    - Update dengan multiple signals (berita, polling, dll)
    - Hasilkan conservative estimate dengan uncertainty buffer
    """
    
    def __init__(self, config=None):
        from bot.config import ProbabilityConfig
        self.config = config or ProbabilityConfig()
        
        # Default signal weights
        self.signal_weights = {
            "news": self.config.news_weight,
            "polling": self.config.polling_weight,
            "price_history": self.config.price_history_weight,
            "volume": self.config.volume_weight,
            "orderbook": self.config.orderbook_weight,
            "source_quality": 0.1,
            "freshness": 0.05,
        }
    
    def logit(self, p: float) -> float:
        """Convert probability ke logit space."""
        # Clamp p untuk menghindari log(0)
        p = max(0.001, min(0.999, p))
        return math.log(p / (1 - p))
    
    def sigmoid(self, z: float) -> float:
        """Convert logit kembali ke probability."""
        return 1 / (1 + math.exp(-z))
    
    def calculate_uncertainty(self, signals: List[Signal], market_data: Dict) -> float:
        """
        Hitung uncertainty berdasarkan:
        - Jumlah sinyal
        - Kualitas sinyal
        - Kondisi market
        """
        base_uncertainty = self.config.uncertainty_buffer
        
        # Kurangi uncertainty jika banyak sinyal berkualitas
        if len(signals) > 3:
            avg_confidence = sum(s.confidence for s in signals) / len(signals)
            if avg_confidence > 0.7:
                base_uncertainty *= 0.8
        
        # Tambah uncertainty jika market tidak likuid
        volume = market_data.get("volume", 0)
        if volume < 10000:
            base_uncertainty *= 1.3
        
        # Tambah uncertainty jika spread lebar
        spread = market_data.get("spread", 0)
        if spread > 0.05:
            base_uncertainty *= 1.2
        
        # Clamp uncertainty
        return max(0.02, min(0.15, base_uncertainty))
    
    def estimate(
        self,
        prior: float,
        signals: List[Signal],
        market_data: Dict = None
    ) -> ProbabilityEstimate:
        """
        Estimasi probabilitas menggunakan Bayesian updating.
        
        Args:
            prior: Probabilitas awal (dari market price atau estimasi manual)
            signals: List sinyal input
            market_data: Data market tambahan (volume, spread, dll)
        
        Returns:
            ProbabilityEstimate dengan probabilitas hasil
        """
        if market_data is None:
            market_data = {}
        
        # Mulai dari prior dalam logit space
        z = self.logit(prior)
        
        # Tambahkan kontribusi setiap sinyal
        signals_used = []
        for signal in signals:
            if signal.name in self.signal_weights:
                weight = self.signal_weights[signal.name] * signal.weight
                # Kontribusi sinyal = weight * value * confidence
                contribution = weight * signal.value * signal.confidence
                z += contribution
                signals_used.append(signal.name)
        
        # Convert kembali ke probability
        q_raw = self.sigmoid(z)
        
        # Hitung uncertainty
        uncertainty = self.calculate_uncertainty(signals, market_data)
        
        # Conservative probability (dikurangi uncertainty)
        q_conservative = max(0.01, min(0.99, q_raw - uncertainty))
        
        # Hitung confidence berdasarkan konsistensi sinyal
        if len(signals) > 0:
            avg_confidence = sum(s.confidence for s in signals) / len(signals)
            confidence = avg_confidence * (1 - uncertainty)
        else:
            confidence = 0.3
        
        return ProbabilityEstimate(
            raw_probability=round(q_raw, 4),
            conservative_probability=round(q_conservative, 4),
            uncertainty=round(uncertainty, 4),
            confidence=round(confidence, 4),
            signals_used=signals_used,
            logit_value=round(z, 4),
        )
    
    def update_with_news(
        self,
        current_estimate: ProbabilityEstimate,
        news_sentiment: float,
        news_confidence: float,
        news_freshness: float = 1.0
    ) -> ProbabilityEstimate:
        """
        Update estimasi dengan berita baru.
        
        Args:
            current_estimate: Estimasi saat ini
            news_sentiment: Sentimen berita (-1 sampai 1)
            news_confidence: Confidence berita (0 sampai 1)
            news_freshness: Kebaruan berita (0 sampai 1, 1 = sangat baru)
        """
        signal = Signal(
            name="news",
            value=news_sentiment,
            confidence=news_confidence * news_freshness,
            weight=self.signal_weights.get("news", 0.25)
        )
        
        return self.estimate(
            prior=current_estimate.raw_probability,
            signals=[signal],
            market_data={"volume": 10000}
        )
    
    def combine_estimates(
        self,
        estimates: List[ProbabilityEstimate],
        weights: List[float] = None
    ) -> ProbabilityEstimate:
        """
        Gabungkan beberapa estimasi (misal dari model berbeda).
        """
        if not estimates:
            return ProbabilityEstimate(
                raw_probability=0.5,
                conservative_probability=0.5,
                uncertainty=0.1,
                confidence=0.3,
                signals_used=[]
            )
        
        if weights is None:
            weights = [1.0] * len(estimates)
        
        # Weighted average
        total_weight = sum(weights)
        q_weighted = sum(
            e.raw_probability * w 
            for e, w in zip(estimates, weights)
        ) / total_weight
        
        avg_uncertainty = sum(e.uncertainty for e in estimates) / len(estimates)
        avg_confidence = sum(e.confidence for e in estimates) / len(estimates)
        
        q_conservative = max(0.01, min(0.99, q_weighted - avg_uncertainty))
        
        all_signals = []
        for e in estimates:
            all_signals.extend(e.signals_used)
        
        return ProbabilityEstimate(
            raw_probability=round(q_weighted, 4),
            conservative_probability=round(q_conservative, 4),
            uncertainty=round(avg_uncertainty, 4),
            confidence=round(avg_confidence, 4),
            signals_used=list(set(all_signals)),
        )
