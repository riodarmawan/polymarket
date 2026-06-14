use crate::storage::database::Database;

#[derive(Debug)]
pub struct Dashboard;

impl Dashboard {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub async fn render(&self, db: &Database, capital: f64) -> anyhow::Result<()> {
        let positions = db.get_open_positions().await?;

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                  POLYMARKET PAPER PORTFOLIO                  ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!(
            "║ Capital: ${:>10.2}                                        ║",
            capital
        );
        println!(
            "║ Open Positions: {:>5}                                       ║",
            positions.len()
        );
        println!("╠══════════════════════════════════════════════════════════════╣");

        if positions.is_empty() {
            println!("║ No open positions                                          ║");
        } else {
            for pos in &positions {
                let pnl = (pos.current_price - pos.entry_price) / pos.entry_price * 100.0;
                println!(
                    "║ {:>4} {:>3} @ ${:.3} -> ${:.3} ({:>+.1}%) {:>10.2}    ║",
                    &pos.id[..4.min(pos.id.len())],
                    pos.side,
                    pos.entry_price,
                    pos.current_price,
                    pnl,
                    pos.size_usd
                );
            }
        }

        println!("╚══════════════════════════════════════════════════════════════╝");
        Ok(())
    }
}
