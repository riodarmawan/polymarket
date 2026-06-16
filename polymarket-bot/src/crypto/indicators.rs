use crate::crypto::binance_ws::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    M5,
    M15,
    H1,
    H4,
    D1,
}

impl Timeframe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::H1 => "1h",
            Timeframe::H4 => "4h",
            Timeframe::D1 => "1d",
        }
    }

    pub fn candle_count(&self) -> usize {
        match self {
            Timeframe::M5 => 5,
            Timeframe::M15 => 15,
            Timeframe::H1 => 60,
            Timeframe::H4 => 240,
            Timeframe::D1 => 1440,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndicatorSet {
    pub timeframe: Timeframe,
    pub ema_short: f64,
    pub ema_long: f64,
    pub adx: f64,
    pub rsi: f64,
    pub trend: Trend,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Trend {
    Bullish,
    Bearish,
    Neutral,
}

pub struct IndicatorEngine {
    ema_short_period: usize,
    ema_long_period: usize,
    adx_period: usize,
    rsi_period: usize,
}

impl IndicatorEngine {
    pub fn new() -> Self {
        Self {
            ema_short_period: 20,
            ema_long_period: 50,
            adx_period: 14,
            rsi_period: 14,
        }
    }

    pub fn calculate(&self, candles: &[Candle], timeframe: Timeframe) -> Option<IndicatorSet> {
        if candles.len() < self.ema_long_period {
            return None;
        }

        let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();

        let ema_short = self.calculate_ema(&closes, self.ema_short_period)?;
        let ema_long = self.calculate_ema(&closes, self.ema_long_period)?;
        let adx = self.calculate_adx(candles, self.adx_period)?;
        let rsi = self.calculate_rsi(&closes, self.rsi_period)?;

        let trend = if ema_short > ema_long && adx > 15.0 {
            Trend::Bullish
        } else if ema_short < ema_long && adx > 15.0 {
            Trend::Bearish
        } else {
            Trend::Neutral
        };

        Some(IndicatorSet {
            timeframe,
            ema_short,
            ema_long,
            adx,
            rsi,
            trend,
        })
    }

    fn calculate_ema(&self, data: &[f64], period: usize) -> Option<f64> {
        if data.len() < period {
            return None;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = data[..period].iter().sum::<f64>() / period as f64;

        for &price in &data[period..] {
            ema = (price - ema) * multiplier + ema;
        }

        Some(ema)
    }

    fn calculate_adx(&self, candles: &[Candle], period: usize) -> Option<f64> {
        if candles.len() < period + 1 {
            return None;
        }

        let mut tr_sum = 0.0;
        let mut plus_dm_sum = 0.0;
        let mut minus_dm_sum = 0.0;

        for i in 1..=period {
            let high = candles[i].high;
            let low = candles[i].low;
            let prev_high = candles[i - 1].high;
            let prev_low = candles[i - 1].low;
            let prev_close = candles[i - 1].close;

            let tr = (high - low)
                .max((high - prev_close).abs())
                .max((low - prev_close).abs());
            let plus_dm = if high - prev_high > prev_low - low && high - prev_high > 0.0 {
                high - prev_high
            } else {
                0.0
            };
            let minus_dm = if prev_low - low > high - prev_high && prev_low - low > 0.0 {
                prev_low - low
            } else {
                0.0
            };

            tr_sum += tr;
            plus_dm_sum += plus_dm;
            minus_dm_sum += minus_dm;
        }

        let plus_di = if tr_sum > 0.0 {
            (plus_dm_sum / tr_sum) * 100.0
        } else {
            0.0
        };
        let minus_di = if tr_sum > 0.0 {
            (minus_dm_sum / tr_sum) * 100.0
        } else {
            0.0
        };

        let di_sum = plus_di + minus_di;
        let dx = if di_sum > 0.0 {
            ((plus_di - minus_di).abs() / di_sum) * 100.0
        } else {
            0.0
        };

        Some(dx)
    }

    fn calculate_rsi(&self, data: &[f64], period: usize) -> Option<f64> {
        if data.len() < period + 1 {
            return None;
        }

        let mut gains = 0.0;
        let mut losses = 0.0;

        for i in 1..=period {
            let change = data[i] - data[i - 1];
            if change > 0.0 {
                gains += change;
            } else {
                losses -= change;
            }
        }

        let avg_gain = gains / period as f64;
        let avg_loss = losses / period as f64;

        if avg_loss == 0.0 {
            return Some(100.0);
        }

        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }
}
