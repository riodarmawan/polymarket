/// Calculate the average execution price considering slippage
pub fn calculate_slippage(
    base_price: f64,
    depth: f64,
    order_size_usd: f64,
    tick_size: f64,
) -> f64 {
    if depth <= 0.0 || order_size_usd <= 0.0 {
        return base_price;
    }

    let shares = order_size_usd / base_price;

    if shares <= depth {
        let slippage_ratio = shares / depth;
        let slippage = slippage_ratio * base_price * 0.1;
        let execution_price = base_price + slippage;
        round_to_tick(execution_price, tick_size)
    } else {
        let excess_shares = shares - depth;
        let slippage_from_depth = base_price * 0.1;
        let slippage_from_excess = (excess_shares / depth) * base_price * 0.2;
        let execution_price = base_price + slippage_from_depth + slippage_from_excess;
        round_to_tick(execution_price.min(base_price * 1.5), tick_size)
    }
}

/// Round price to nearest tick size
pub fn round_to_tick(price: f64, tick_size: f64) -> f64 {
    if tick_size <= 0.0 {
        return price;
    }
    (price / tick_size).round() * tick_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_slippage_small_order() {
        let price = calculate_slippage(0.5, 1000.0, 10.0, 0.01);
        assert!(price >= 0.5 && price < 0.55);
    }

    #[test]
    fn test_slippage_large_order() {
        let price = calculate_slippage(0.5, 100.0, 200.0, 0.01);
        assert!(price > 0.5);
    }

    #[test]
    fn test_tick_rounding() {
        let price = round_to_tick(0.5123, 0.01);
        assert!((price - 0.51).abs() < 0.001);
    }
}
