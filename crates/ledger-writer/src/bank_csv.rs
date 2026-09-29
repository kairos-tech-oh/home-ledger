//! Reading a card or bank export into statement charges.
//!
//! Every bank writes its own CSV. What they share is a date, a description and
//! an amount, either signed in one column or split into debit and credit
//! columns; what differs is the header names, the date format, and which sign
//! a purchase carries (Chase writes purchases negative, American Express and
//! Discover positive). So the columns are found by name, the sign is worked
//! out from the file, and the screen can override any of it when a guess is
//! wrong.
//!
//! Nothing here writes. The result is a preview; the person chooses what to
//! add, and `Op::ReconcileImport` adds it.

use ledger_domain::Money;
use ledger_math::calendar::Day;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Larger than any real statement export, small enough to refuse a wrong file.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ROWS: usize = 5_000;

/// Which column holds what. Indexes into the header row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Mapping {
    pub date: Option<usize>,
    pub description: Option<usize>,
    /// One signed amount column...
    pub amount: Option<usize>,
    /// ...or separate columns for money out and money in.
    pub debit: Option<usize>,
    pub credit: Option<usize>,
    pub category: Option<usize>,
    /// "Sale", "Payment", "Return": a bank's own word for the transaction.
    pub kind: Option<usize>,
    /// In a single amount column, whether a purchase is written negative.
    pub charges_negative: bool,
    /// Dates written day first (31/12/2026) rather than month first.
    pub day_first: bool,
}

/// One line of the file, read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    /// Line number in the file, counting from 1, for "line 14 could not be read".
    pub line: usize,
    /// `yyyy-mm-dd`, empty when the date could not be read.
    pub date: String,
    pub description: String,
    pub category: String,
    pub kind: String,
    /// Positive for a purchase.
    pub amount: Money,
    /// A purchase, as opposed to a payment, refund or credit.
    pub charge: bool,
    /// Why this line would not be added, empty when it would.
    pub problem: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Read {
    pub headers: Vec<String>,
    pub mapping: Mapping,
    /// False when the file had no header row and columns are numbered.
    pub had_headers: bool,
    pub rows: Vec<Transaction>,
    /// What the guess could not settle, said plainly.
    pub notes: Vec<String>,
}

/// Split CSV text into rows of fields: quoted fields, doubled quotes, commas
/// and line breaks inside quotes, CRLF or LF, and a leading byte-order mark.
pub fn rows(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
            }
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                out.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        out.push(row);
    }
    out.into_iter()
        .map(|r| {
            r.into_iter()
                .map(|f| f.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|r| r.iter().any(|f| !f.is_empty()))
        .collect()
}

/// An amount as banks write it: `-5.39`, `$1,234.56`, `(12.00)`, `12.00-`,
/// `USD 3.10`. None for anything else, including an empty field.
pub fn amount(text: &str) -> Option<Decimal> {
    let mut t: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '$')
        .collect();
    t = t
        .trim_start_matches("USD")
        .trim_end_matches("USD")
        .to_string();
    if t.is_empty() {
        return None;
    }
    let mut negative = false;
    if t.starts_with('(') && t.ends_with(')') {
        negative = true;
        t = t[1..t.len() - 1].to_string();
    }
    if let Some(rest) = t.strip_suffix('-') {
        negative = !negative;
        t = rest.to_string();
    }
    if let Some(rest) = t.strip_prefix('+') {
        t = rest.to_string();
    }
    let value = Decimal::from_str(&t).ok()?;
    Some(if negative { -value } else { value })
}

/// A date as banks write it: `09/28/2026`, `9/8/26`, `2026-09-28`,
/// `2026/09/28`, or day first when asked. Only a date that exists.
pub fn date(text: &str, day_first: bool) -> Option<Day> {
    let t = text.split_whitespace().next()?;
    let parts: Vec<&str> = t.split(['/', '-', '.']).collect();
    if parts.len() != 3 {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let (y, m, d) = if parts[0].len() == 4 {
        (num(parts[0])?, num(parts[1])?, num(parts[2])?)
    } else {
        let (a, b, y) = (num(parts[0])?, num(parts[1])?, num(parts[2])?);
        let y = if parts[2].len() == 2 { 2000 + y } else { y };
        if day_first { (y, b, a) } else { (y, a, b) }
    };
    Day::parse(&format!("{y:04}-{m:02}-{d:02}"))
}

fn find(headers: &[String], names: &[&str]) -> Option<usize> {
    let lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    names
        .iter()
        .find_map(|name| lower.iter().position(|h| h == name))
}

/// Header names as the common US issuers write them, most specific first.
fn guess(headers: &[String]) -> Mapping {
    Mapping {
        date: find(
            headers,
            &[
                "transaction date",
                "trans. date",
                "trans date",
                "date",
                "posted date",
                "post date",
                "posting date",
            ],
        ),
        description: find(
            headers,
            &[
                "description",
                "transaction description",
                "payee",
                "merchant",
                "merchant name",
                "name",
                "details",
                "memo",
            ],
        ),
        amount: find(headers, &["amount", "transaction amount", "amount (usd)"]),
        debit: find(
            headers,
            &[
                "debit",
                "debits",
                "withdrawal",
                "withdrawals",
                "charge",
                "charges",
            ],
        ),
        credit: find(headers, &["credit", "credits", "deposit", "deposits"]),
        category: find(headers, &["category", "transaction category"]),
        kind: find(headers, &["type", "transaction type"]),
        charges_negative: true,
        day_first: false,
    }
}

/// A bank's own word for a purchase, where it gives one.
fn is_purchase_word(kind: &str) -> Option<bool> {
    match kind.trim().to_lowercase().as_str() {
        "sale" | "purchase" | "debit" | "fee" | "interest" => Some(true),
        "payment" | "return" | "refund" | "credit" | "adjustment" => Some(false),
        _ => None,
    }
}

/// Read a file. `wanted` replaces the guessed columns and sign; the header row
/// is found either way.
pub fn read(text: &str, wanted: Option<Mapping>) -> Result<Read, String> {
    if text.len() > MAX_BYTES {
        return Err("that file is too large to be a statement export".into());
    }
    let mut all = rows(text);
    if all.is_empty() {
        return Err("that file has no rows".into());
    }
    all.truncate(MAX_ROWS + 1);
    let mut notes = Vec::new();

    // A header row is one where no field reads as an amount or a date. Wells
    // Fargo, for one, writes none.
    let had_headers = !all[0]
        .iter()
        .any(|f| amount(f).is_some() || date(f, false).is_some());
    let width = all.iter().map(Vec::len).max().unwrap_or(0);
    let headers: Vec<String> = if had_headers {
        all.remove(0)
    } else {
        (1..=width).map(|i| format!("Column {i}")).collect()
    };

    let mut mapping = match wanted {
        Some(m) => m,
        None => {
            let mut m = if had_headers {
                guess(&headers)
            } else {
                Mapping::default()
            };
            if !had_headers {
                // Numbered columns: the first date, the first amount, and the
                // longest text are the likeliest date, amount and description.
                let sample = &all[..all.len().min(20)];
                let share = |i: usize, test: &dyn Fn(&str) -> bool| {
                    sample
                        .iter()
                        .filter(|r| r.get(i).is_some_and(|f| test(f)))
                        .count()
                };
                let enough = sample.len().div_ceil(2);
                m.date = (0..width).find(|&i| share(i, &|f| date(f, false).is_some()) >= enough);
                m.amount = (0..width)
                    .find(|&i| Some(i) != m.date && share(i, &|f| amount(f).is_some()) >= enough);
                m.description = (0..width)
                    .filter(|&i| Some(i) != m.date && Some(i) != m.amount)
                    .max_by_key(|&i| {
                        sample
                            .iter()
                            .map(|r| r.get(i).map_or(0, String::len))
                            .sum::<usize>()
                    });
                notes.push("The file has no header row, so its columns were guessed.".into());
            }
            // A signed column wins over debit/credit ones only when there is
            // no pair to use.
            if m.debit.is_some() && m.credit.is_some() {
                m.amount = None;
            }
            m.charges_negative = sign_of_charges(&all, &m, &mut notes);
            m
        }
    };
    if mapping.amount.is_some() {
        mapping.debit = None;
        mapping.credit = None;
    }

    if mapping.date.is_none() {
        notes.push("No date column was found; choose one below.".into());
    }
    if mapping.description.is_none() {
        notes.push("No description column was found; choose one below.".into());
    }
    if mapping.amount.is_none() && mapping.debit.is_none() {
        notes.push("No amount column was found; choose one below.".into());
    }

    let field = |row: &[String], at: Option<usize>| -> String {
        at.and_then(|i| row.get(i)).cloned().unwrap_or_default()
    };
    let first_line = if had_headers { 2 } else { 1 };
    let rows = all
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let kind = field(row, mapping.kind);
            let raw_date = field(row, mapping.date);
            let day = date(&raw_date, mapping.day_first);
            let (value, charge) = match (mapping.amount, mapping.debit) {
                (Some(_), _) => match amount(&field(row, mapping.amount)) {
                    Some(v) => {
                        let spent = if mapping.charges_negative { -v } else { v };
                        (Some(spent.abs()), spent > Decimal::ZERO)
                    }
                    None => (None, false),
                },
                (None, Some(_)) => {
                    let out = amount(&field(row, mapping.debit)).map(|v| v.abs());
                    let back = amount(&field(row, mapping.credit)).map(|v| v.abs());
                    match (out, back) {
                        (Some(v), _) if v > Decimal::ZERO => (Some(v), true),
                        (_, Some(v)) => (Some(v), false),
                        _ => (None, false),
                    }
                }
                _ => (None, false),
            };
            let problem = if value.is_none() {
                "no amount could be read".to_string()
            } else if !charge {
                "a payment, refund or credit, not a purchase".to_string()
            } else if day.is_none() {
                "no date could be read".to_string()
            } else {
                String::new()
            };
            Transaction {
                line: first_line + i,
                date: day.map(Day::iso).unwrap_or_default(),
                description: ledger_domain::plain(&field(row, mapping.description), 120),
                category: ledger_domain::plain(&field(row, mapping.category), 80),
                kind,
                amount: Money::new(value.unwrap_or_default()),
                charge,
                problem,
            }
        })
        .collect();

    Ok(Read {
        headers,
        mapping,
        had_headers,
        rows,
        notes,
    })
}

/// Which way purchases are signed in a single amount column. A bank that
/// names its transactions says so; otherwise purchases are what most rows are.
fn sign_of_charges(rows: &[Vec<String>], m: &Mapping, notes: &mut Vec<String>) -> bool {
    let Some(col) = m.amount else {
        return true;
    };
    let signed = |row: &Vec<String>| {
        row.get(col)
            .and_then(|f| amount(f))
            .filter(|v| !v.is_zero())
    };
    if let Some(kind) = m.kind {
        let (mut neg, mut pos) = (0, 0);
        for row in rows {
            let purchase = row.get(kind).and_then(|k| is_purchase_word(k));
            if let (Some(true), Some(v)) = (purchase, signed(row)) {
                if v.is_sign_negative() {
                    neg += 1
                } else {
                    pos += 1
                }
            }
        }
        if neg + pos > 0 {
            return neg >= pos;
        }
    }
    let (neg, pos) = rows.iter().filter_map(signed).fold((0, 0), |(n, p), v| {
        if v.is_sign_negative() {
            (n + 1, p)
        } else {
            (n, p + 1)
        }
    });
    if neg == pos && neg > 0 {
        notes.push(
            "As many amounts are negative as positive; check which way purchases are signed."
                .into(),
        );
    }
    neg >= pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// The Chase Sapphire export this was built for, with a payment and a
    /// return added the way Chase writes them.
    const CHASE: &str = "Transaction Date,Post Date,Description,Category,Type,Amount,Memo\r\n\
09/28/2026,09/28/2026,GOOGLE *YouTube TV,Shopping,Sale,-5.39,\r\n\
09/27/2026,09/28/2026,COSTCO WHSE #0632,Shopping,Sale,-282.18,\r\n\
09/25/2026,09/27/2026,MCDONALD'S F30010,Food & Drink,Sale,-22.04,\r\n\
09/26/2026,09/27/2026,KROGER #920,Groceries,Sale,-48.59,\r\n\
09/20/2026,09/20/2026,Payment Thank You-Mobile,,Payment,1500.00,\r\n\
09/19/2026,09/20/2026,PETCO 2823,Shopping,Return,25.36,\r\n";

    #[test]
    fn a_chase_export_reads_as_charges_with_payments_and_returns_set_aside() {
        let r = read(CHASE, None).unwrap();
        assert!(r.had_headers);
        assert_eq!(r.mapping.date, Some(0), "transaction date, not post date");
        assert_eq!(r.mapping.description, Some(2));
        assert_eq!(r.mapping.amount, Some(5));
        assert_eq!(r.mapping.kind, Some(4));
        assert!(r.mapping.charges_negative);
        let charges: Vec<_> = r.rows.iter().filter(|t| t.problem.is_empty()).collect();
        assert_eq!(charges.len(), 4);
        assert_eq!(charges[0].date, "2026-09-28");
        assert_eq!(charges[0].description, "GOOGLE *YouTube TV");
        assert_eq!(charges[0].amount, Money::new(dec!(5.39)));
        assert_eq!(charges[1].category, "Shopping");
        let payment = &r.rows[4];
        assert!(!payment.charge);
        assert_eq!(payment.line, 6);
        assert!(!r.rows[5].charge, "a return is not a purchase");
    }

    #[test]
    fn an_amex_export_writes_purchases_positive() {
        let text = "Date,Description,Amount\n09/03/2026,WHOLE FOODS,54.10\n09/04/2026,SHELL OIL,41.00\n\
09/05/2026,AUTOPAY PAYMENT - THANK YOU,-500.00\n";
        let r = read(text, None).unwrap();
        assert!(!r.mapping.charges_negative);
        let ok: Vec<_> = r
            .rows
            .iter()
            .filter(|t| t.problem.is_empty())
            .map(|t| t.amount)
            .collect();
        assert_eq!(ok, [Money::new(dec!(54.10)), Money::from(41)]);
    }

    #[test]
    fn a_capital_one_export_splits_debit_and_credit() {
        let text = "Transaction Date,Posted Date,Card No.,Description,Category,Debit,Credit\n\
2026-09-01,2026-09-02,1234,TARGET,Merchandise,63.21,\n\
2026-09-03,2026-09-03,1234,CAPITAL ONE MOBILE PYMT,Payment/Credit,,250.00\n";
        let r = read(text, None).unwrap();
        assert_eq!(
            (r.mapping.debit, r.mapping.credit, r.mapping.amount),
            (Some(5), Some(6), None)
        );
        assert!(r.rows[0].charge && r.rows[0].problem.is_empty());
        assert_eq!(r.rows[0].amount, Money::new(dec!(63.21)));
        assert!(!r.rows[1].charge);
    }

    #[test]
    fn a_file_with_no_header_row_has_its_columns_guessed() {
        // Wells Fargo: date, amount, *, blank, description.
        let text = "\"09/10/2026\",\"-18.75\",\"*\",\"\",\"PANERA BREAD #601\"\n\
\"09/11/2026\",\"-6.20\",\"*\",\"\",\"STARBUCKS STORE 1\"\n";
        let r = read(text, None).unwrap();
        assert!(!r.had_headers);
        assert_eq!(
            (r.mapping.date, r.mapping.amount, r.mapping.description),
            (Some(0), Some(1), Some(4))
        );
        assert_eq!(r.rows[0].description, "PANERA BREAD #601");
        assert_eq!(r.rows[0].amount, Money::new(dec!(18.75)));
        assert_eq!(r.rows[0].line, 1);
        assert!(!r.notes.is_empty());
    }

    #[test]
    fn a_chosen_mapping_replaces_the_guess() {
        let mut m = read(CHASE, None).unwrap().mapping;
        m.date = Some(1);
        m.charges_negative = false;
        let r = read(CHASE, Some(m)).unwrap();
        assert_eq!(r.rows[0].date, "2026-09-28");
        assert_eq!(r.rows[1].date, "2026-09-28", "the post date now");
        // Flipped: the payment reads as the purchase.
        assert!(r.rows[4].charge && !r.rows[0].charge);
    }

    #[test]
    fn quoted_fields_keep_their_commas_and_quotes() {
        let got = rows("a,\"b, c\",\"say \"\"hi\"\"\"\r\n\"multi\nline\",x,\n");
        assert_eq!(
            got,
            [
                vec!["a", "b, c", "say \"hi\""],
                vec!["multi\nline", "x", ""]
            ]
        );
        assert_eq!(
            rows("\u{feff}h1,h2\n\n1,2"),
            [vec!["h1", "h2"], vec!["1", "2"]]
        );
    }

    #[test]
    fn amounts_as_banks_write_them() {
        for (text, want) in [
            ("-5.39", dec!(-5.39)),
            ("$1,234.56", dec!(1234.56)),
            ("(12.00)", dec!(-12.00)),
            ("12.00-", dec!(-12.00)),
            ("+3", dec!(3)),
            ("USD 3.10", dec!(3.10)),
        ] {
            assert_eq!(amount(text), Some(want), "{text}");
        }
        for bad in ["", "abc", "1.2.3"] {
            assert_eq!(amount(bad), None, "{bad}");
        }
    }

    #[test]
    fn dates_as_banks_write_them() {
        for (text, want) in [
            ("09/28/2026", "2026-09-28"),
            ("9/8/26", "2026-09-08"),
            ("2026-09-28", "2026-09-28"),
            ("2026/09/28", "2026-09-28"),
            ("09/28/2026 14:03", "2026-09-28"),
        ] {
            assert_eq!(
                date(text, false).map(Day::iso).as_deref(),
                Some(want),
                "{text}"
            );
        }
        assert_eq!(
            date("28/09/2026", true).map(Day::iso).as_deref(),
            Some("2026-09-28")
        );
        assert_eq!(date("28/09/2026", false), None, "no 28th month");
        assert_eq!(date("02/30/2026", false), None);
    }

    #[test]
    fn nothing_to_read_is_said_plainly() {
        assert!(read("", None).is_err());
        assert!(read(&"x".repeat(MAX_BYTES + 1), None).is_err());
        let r = read("Foo,Bar\n1,2\n", None);
        // "1" and "2" read as amounts, so this is taken as having no header.
        assert!(r.is_ok());
        let r = read("Foo,Bar\nhello,world\n", None).unwrap();
        assert!(r.notes.iter().any(|n| n.contains("No date column")));
        assert!(r.rows.iter().all(|t| !t.problem.is_empty()));
    }
}
