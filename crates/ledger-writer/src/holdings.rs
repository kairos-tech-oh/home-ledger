//! Trades on a holding. Adding, editing, pricing by hand and deleting one go
//! through `Op::Set` and `Op::Delete` like every other record.

use crate::{Side, WriteError, Writer, now_iso};
use ledger_domain::records::{AuditChange, AuditEntry, Trade, caps};
use ledger_domain::text::{LINE_NOTES_MAX, plain, valid_id};
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;

/// A holding stored at four places can be sold whole without a crumb refusing it.
const SELL_SLACK: Decimal = Decimal::from_parts(1, 0, 0, false, 4);

/// A share count as a person reads it: no trailing zeros, up to four places.
fn shares(value: Money) -> String {
    value.inner().normalize().to_string()
}

impl Writer {
    pub(crate) fn trade(
        &self,
        ledger: &mut Ledger,
        id: &str,
        side: Side,
        quantity: Money,
        price: Money,
        notes: &str,
    ) -> Result<AuditEntry, WriteError> {
        let wanted = valid_id(id);
        let Some(row) = ledger.investments.iter_mut().find(|h| h.id == wanted) else {
            return Err(WriteError::Missing("no such holding".into()));
        };
        let quantity = Money::price(quantity.inner());
        let price = Money::price(price.inner());
        if quantity <= Money::ZERO {
            return Err(WriteError::Refused("the quantity must be positive".into()));
        }
        if price.is_negative() {
            return Err(WriteError::Refused("the price cannot be negative".into()));
        }

        let held = row.quantity;
        let was_avg = row.avg_cost;
        // Average-cost accounting: a buy re-averages, a sell only rebases.
        let (after, avg) = match side {
            Side::Buy => {
                let after = Money::price(held.inner() + quantity.inner());
                let basis = was_avg.inner() * held.inner() + quantity.inner() * price.inner();
                let avg = if after > Money::ZERO {
                    Money::price(basis / after.inner())
                } else {
                    Money::ZERO
                };
                (after, avg)
            }
            Side::Sell => {
                if quantity.inner() > held.inner() + SELL_SLACK {
                    return Err(WriteError::Refused(format!(
                        "only {} held, cannot sell {}",
                        shares(held),
                        shares(quantity)
                    )));
                }
                let after = Money::price(held.inner() - quantity.inner()).floor_at_zero();
                (after, was_avg)
            }
        };

        row.quantity = after;
        row.avg_cost = avg;
        row.cost_basis = Some(Money::new(avg.inner() * after.inner()));
        row.trades.push(Trade {
            id: ledger_domain::text::new_id(),
            kind: match side {
                Side::Buy => "buy",
                Side::Sell => "sell",
            }
            .into(),
            quantity,
            price,
            at: now_iso(),
            notes: plain(notes, LINE_NOTES_MAX),
        });
        let excess = row.trades.len().saturating_sub(caps::TRADES);
        row.trades.drain(..excess);

        // The prototype logged the new average as "from"; this logs the old one.
        let changes = vec![
            AuditChange {
                field: "quantity".into(),
                from: shares(held),
                to: shares(after),
            },
            AuditChange {
                field: "average cost".into(),
                from: was_avg.to_string(),
                to: avg.to_string(),
            },
        ];
        Ok(self.finish(
            AuditEntry {
                action: match side {
                    Side::Buy => "Add",
                    Side::Sell => "Remove",
                }
                .into(),
                subject: "holding".into(),
                name: row.name.clone(),
                amount: Some(Money::new(quantity.inner() * price.inner())),
                changes,
                ..Default::default()
            },
            match side {
                Side::Buy => "holding-buy",
                Side::Sell => "holding-sell",
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Kind, Op, Side, WriteError, Writer};
    use ledger_domain::records::Holding;
    use ledger_domain::{Ledger, Money};
    use rust_decimal::Decimal;
    use serde_json::{Value, json};
    use std::str::FromStr;

    fn writer() -> Writer {
        Writer::new("test")
    }

    fn money(text: &str) -> Money {
        Money::price(Decimal::from_str(text).unwrap())
    }

    fn set(ledger: &mut Ledger, id: &str, record: Value) -> String {
        writer()
            .apply(
                ledger,
                &Op::Set {
                    kind: Kind::Holding,
                    id: id.into(),
                    record,
                },
            )
            .expect("stored");
        if id.is_empty() {
            ledger.investments.last().unwrap().id.clone()
        } else {
            id.into()
        }
    }

    fn trade(
        ledger: &mut Ledger,
        id: &str,
        side: Side,
        quantity: &str,
        price: &str,
    ) -> Result<(), WriteError> {
        writer()
            .apply(
                ledger,
                &Op::Trade {
                    id: id.into(),
                    side,
                    quantity: money(quantity),
                    price: money(price),
                    notes: String::new(),
                },
            )
            .map(|_| ())
    }

    fn held<'a>(ledger: &'a Ledger, id: &str) -> &'a Holding {
        ledger.investments.iter().find(|h| h.id == id).unwrap()
    }

    // The cases below are check-helper.py's, with its inputs and answers.

    #[test]
    fn a_holding_derives_its_average_cost_and_keeps_four_decimal_prices() {
        let mut ledger = Ledger::default();
        let id = set(
            &mut ledger,
            "",
            json!({ "name": "Trade Test", "ticker": "vti", "type": "Stock/Security",
                    "quantity": 100, "costBasis": 25000, "price": 300.1234 }),
        );
        let h = held(&ledger, &id);
        assert_eq!(h.avg_cost, Money::from(250));
        assert_eq!(h.price, Some(money("300.1234")));
        assert_eq!(h.ticker, "VTI");
        assert!(h.trades.is_empty());
        assert_eq!(h.bucket_id, "");
    }

    #[test]
    fn a_buy_re_averages_and_a_sell_only_rebases() {
        let mut ledger = Ledger::default();
        let id = set(
            &mut ledger,
            "",
            json!({ "name": "Trade Test", "quantity": 100, "costBasis": 25000 }),
        );

        // 100 @ 250 plus 50 @ 320 is 41,000 over 150 shares.
        trade(&mut ledger, &id, Side::Buy, "50", "320").unwrap();
        let h = held(&ledger, &id);
        assert_eq!(
            (h.quantity, h.cost_basis, h.avg_cost),
            (
                Money::from(150),
                Some(Money::from(41000)),
                money("273.3333")
            )
        );

        trade(&mut ledger, &id, Side::Sell, "30", "340").unwrap();
        let h = held(&ledger, &id);
        assert_eq!(
            (h.quantity, h.cost_basis, h.avg_cost),
            (
                Money::from(120),
                Some(Money::from(32800)),
                money("273.3333")
            )
        );
        let kinds: Vec<&str> = h.trades.iter().map(|t| t.kind.as_str()).collect();
        assert_eq!(kinds, ["buy", "sell"]);
        assert!(
            h.trades.iter().all(|t| t.id.len() == 32),
            "a trade has no id"
        );
    }

    #[test]
    fn selling_more_than_is_held_is_refused_and_nothing_moves() {
        let mut ledger = Ledger::default();
        let id = set(
            &mut ledger,
            "",
            json!({ "name": "Trade Test", "quantity": 120, "costBasis": 32800 }),
        );
        let refused = trade(&mut ledger, &id, Side::Sell, "9999", "1");
        assert!(matches!(refused, Err(WriteError::Refused(_))));
        assert_eq!(held(&ledger, &id).quantity, Money::from(120));
        assert!(held(&ledger, &id).trades.is_empty());
    }

    #[test]
    fn a_negative_trade_quantity_is_refused() {
        let mut ledger = Ledger::default();
        let id = set(
            &mut ledger,
            "",
            json!({ "name": "Trade Test", "quantity": 1 }),
        );
        assert!(trade(&mut ledger, &id, Side::Buy, "-5", "10").is_err());
    }

    #[test]
    fn selling_the_whole_holding_empties_it_exactly() {
        let mut ledger = Ledger::default();
        let id = set(
            &mut ledger,
            "",
            json!({ "name": "Crumb", "quantity": 0.3333, "costBasis": 10 }),
        );
        assert_eq!(
            held(&ledger, &id).quantity,
            money("0.3333"),
            "quantity lost places"
        );
        trade(&mut ledger, &id, Side::Sell, "0.3333", "1").unwrap();
        assert_eq!(held(&ledger, &id).quantity, Money::ZERO);
    }

    #[test]
    fn figures_sent_as_strings_keep_every_place() {
        // The interface sends amounts as strings, never as JavaScript numbers.
        let mut ledger = Ledger::default();
        let id = set(
            &mut ledger,
            "",
            json!({ "name": "Crumb", "quantity": "0.3333", "costBasis": "10", "price": "300.1234" }),
        );
        let h = held(&ledger, &id);
        assert_eq!(
            (h.quantity, h.price),
            (money("0.3333"), Some(money("300.1234")))
        );

        let op: Op = serde_json::from_value(json!({
            "op": "trade", "id": id, "side": "buy", "quantity": "0.1234", "price": "1.5", "notes": ""
        }))
        .unwrap();
        writer().apply(&mut ledger, &op).unwrap();
        assert_eq!(held(&ledger, &id).quantity, money("0.4567"));
    }

    #[test]
    fn a_hand_set_class_is_kept_and_an_unknown_one_dropped() {
        let mut ledger = Ledger::default();
        let id = set(
            &mut ledger,
            "",
            json!({ "name": "Class Test", "assetClass": "us-large", "sector": "Technology" }),
        );
        assert_eq!(held(&ledger, &id).asset_class, "us-large");
        assert_eq!(held(&ledger, &id).sector, "Technology");
        set(&mut ledger, &id, json!({ "assetClass": "moon-rocks" }));
        assert_eq!(held(&ledger, &id).asset_class, "");
    }

    #[test]
    fn a_price_set_by_hand_sticks_and_a_quote_leaves_it_alone() {
        let mut ledger = Ledger::default();
        let id = set(&mut ledger, "", json!({ "name": "Gold", "quantity": 2 }));
        set(
            &mut ledger,
            &id,
            json!({ "price": 2400.5, "fixedPrice": true }),
        );
        writer()
            .apply(
                &mut ledger,
                &Op::PriceHoldings {
                    priced: vec![crate::Priced {
                        id: id.clone(),
                        price: Money::from(1),
                    }],
                    stale: vec![],
                },
            )
            .ok();
        assert_eq!(held(&ledger, &id).price, Some(money("2400.5")));
    }

    #[test]
    fn deleting_a_holding_removes_only_it() {
        let mut ledger = Ledger::default();
        let a = set(&mut ledger, "", json!({ "name": "A" }));
        set(&mut ledger, "", json!({ "name": "B" }));
        writer()
            .apply(
                &mut ledger,
                &Op::Delete {
                    kind: Kind::Holding,
                    id: a,
                },
            )
            .unwrap();
        let names: Vec<&str> = ledger.investments.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["B"]);
    }

    #[test]
    fn a_trade_survives_a_round_trip_with_its_id() {
        // The trade record had no id field, so a document's trade ids were
        // silently dropped on every write. No real holding had trades yet.
        let raw = json!({ "investments": [ { "id": "a".repeat(32), "name": "X",
            "trades": [ { "id": "b".repeat(32), "kind": "buy", "quantity": 1, "price": 2,
                          "at": "2026-01-01T00:00:00Z", "notes": "" } ] } ] });
        let ledger = Ledger::from_bytes(raw.to_string().as_bytes()).unwrap();
        let back: Value = serde_json::from_slice(&ledger.to_bytes().unwrap()).unwrap();
        assert_eq!(
            back["investments"][0]["trades"][0]["id"],
            json!("b".repeat(32))
        );
    }
}
