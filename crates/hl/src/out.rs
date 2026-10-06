//! What `hl` prints, and how it exits.
//!
//! Every command can print its answer as a table for a person or, with
//! `--json`, as the same structure the desktop app is given, for a script.

use serde::Serialize;
use std::fmt;
use std::process::ExitCode;

/// Why a command failed, which decides its exit code.
#[derive(Debug)]
pub enum Failure {
    /// Something went wrong: 1.
    Error(String),
    /// The command was used wrongly: 2.
    Usage(String),
    /// The ledger is encrypted and this machine is locked: 3.
    Locked,
    /// Saved here and queued, but the store could not be reached: 4.
    Queued(String),
    /// A rule said no, such as a locked bucket: 5.
    Refused(String),
}

impl Failure {
    pub fn code(&self) -> u8 {
        match self {
            Failure::Error(_) => 1,
            Failure::Usage(_) => 2,
            Failure::Locked => 3,
            Failure::Queued(_) => 4,
            Failure::Refused(_) => 5,
        }
    }

    /// From an error the app gave, telling a locked ledger apart.
    pub fn from_app(message: impl fmt::Display) -> Failure {
        let text = message.to_string();
        if text.contains("encrypted") {
            Failure::Locked
        } else {
            Failure::Error(text)
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Error(m) | Failure::Usage(m) | Failure::Refused(m) => f.write_str(m),
            Failure::Locked => f.write_str(
                "the ledger is encrypted and this machine is locked; run `hl unlock`, \
                 or set LEDGER_PASSPHRASE",
            ),
            Failure::Queued(m) => write!(f, "saved on this machine, not yet synced: {m}"),
        }
    }
}

pub type Outcome = Result<(), Failure>;

pub fn exit(outcome: Outcome) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("hl: {failure}");
            ExitCode::from(failure.code())
        }
    }
}

/// `$1,234.56`, `-$12.00`. Amounts arrive as the strings the app computed.
pub fn money(value: &str) -> String {
    let negative = value.starts_with('-');
    let digits = value.trim_start_matches('-');
    let (whole, cents) = match digits.split_once('.') {
        Some((w, c)) => (w, format!("{c:0<2}")),
        None => (digits, "00".into()),
    };
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!(
        "{}${grouped}.{}",
        if negative { "-" } else { "" },
        &cents[..2]
    )
}

pub fn maybe_money(value: &Option<String>) -> String {
    value.as_deref().map(money).unwrap_or_else(|| "—".into())
}

/// A table: a header, rows, and which columns hold figures (right-aligned).
pub struct Table {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
    figures: Vec<usize>,
}

impl Table {
    pub fn new(header: &[&str]) -> Table {
        Table {
            header: header.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
            figures: Vec::new(),
        }
    }

    pub fn figures(mut self, columns: &[usize]) -> Table {
        self.figures = columns.to_vec();
        self
    }

    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    pub fn print(&self) {
        if self.rows.is_empty() {
            println!("(none)");
            return;
        }
        let width = |i: usize| {
            self.rows
                .iter()
                .map(|r| r.get(i).map_or(0, |c| c.chars().count()))
                .chain(std::iter::once(self.header[i].chars().count()))
                .max()
                .unwrap_or(0)
        };
        let widths: Vec<usize> = (0..self.header.len()).map(width).collect();
        let line = |cells: &[String]| {
            let parts: Vec<String> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    if self.figures.contains(&i) {
                        format!("{c:>w$}", w = widths[i])
                    } else {
                        format!("{c:<w$}", w = widths[i])
                    }
                })
                .collect();
            println!("{}", parts.join("  ").trim_end());
        };
        line(&self.header);
        line(&widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>());
        for r in &self.rows {
            line(r);
        }
    }
}

/// Prints `value` as JSON when asked, otherwise runs `text` to print it for a
/// person.
pub fn show<T: Serialize>(json: bool, value: &T, text: impl FnOnce(&T)) -> Outcome {
    if json {
        let body =
            serde_json::to_string_pretty(value).map_err(|e| Failure::Error(e.to_string()))?;
        println!("{body}");
    } else {
        text(value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_read_as_money() {
        assert_eq!(money("1234.5"), "$1,234.50");
        assert_eq!(money("-12"), "-$12.00");
        assert_eq!(money("0.00"), "$0.00");
        assert_eq!(money("195494.00"), "$195,494.00");
        assert_eq!(money("1000000.10"), "$1,000,000.10");
    }

    #[test]
    fn each_failure_has_its_own_exit_code() {
        let codes: Vec<u8> = [
            Failure::Error(String::new()),
            Failure::Usage(String::new()),
            Failure::Locked,
            Failure::Queued(String::new()),
            Failure::Refused(String::new()),
        ]
        .iter()
        .map(Failure::code)
        .collect();
        assert_eq!(codes, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn an_encrypted_ledger_reads_as_locked() {
        assert!(matches!(
            Failure::from_app("the ledger is encrypted; enter the passphrase"),
            Failure::Locked
        ));
        assert!(matches!(
            Failure::from_app("no such bucket"),
            Failure::Error(_)
        ));
    }
}
