//! `hl`: Home Ledger from the command line.
//!
//! Every figure comes from the same code the app's screens use, and every
//! edit goes through the same rules and lands in the same history, signed
//! with this machine and `cli`. On a machine with the app, `hl` uses the
//! app's own setup; on one without, `hl init` sets it up.

// Printing to a reader that has gone away, as in `hl history | head`, stops
// quietly instead of panicking: the standard behaviour for a command line
// tool, and what a script piping into another one expects. Defined before
// the modules so it replaces the standard macros in all of them.
macro_rules! println {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        if writeln!(std::io::stdout(), $($t)*).is_err() {
            std::process::exit(0);
        }
    }};
}
macro_rules! print {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        if write!(std::io::stdout(), $($t)*).is_err() {
            std::process::exit(0);
        }
    }};
}

mod alias;
mod bank;
mod install_path;
mod out;
mod read;
mod session;
mod setup;
mod write;

use clap::{Parser, Subcommand};
use out::{Failure, Outcome};
use session::Session;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "hl",
    version,
    about = "Home Ledger from the command line",
    after_help = "Exit codes: 0 done, 1 error, 2 wrong usage, 3 locked (run `hl unlock`), \
                  4 saved here but not yet synced, 5 refused by a rule."
)]
struct Cli {
    /// Print the answer as JSON, for scripts.
    #[arg(long, global = true)]
    json: bool,
    /// Show what an edit would change, and change nothing.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Label this script in the history, such as "nightly-import".
    #[arg(long, global = true, value_name = "LABEL")]
    via: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The stores, sync, encryption and this machine's name.
    Status,
    /// Push edits waiting on this machine to the store now.
    Sync,
    /// Set up a machine that has no Home Ledger app.
    Init {
        /// What this machine is called in the history.
        #[arg(long)]
        name: String,
        /// Keep the ledger in a file at this path.
        #[arg(long, conflicts_with = "s3_bucket")]
        local: Option<String>,
        #[arg(long)]
        s3_bucket: Option<String>,
        #[arg(long, default_value = "ledger/ledger.json")]
        s3_key: String,
        #[arg(long)]
        s3_region: Option<String>,
        /// For an S3-compatible service other than AWS.
        #[arg(long)]
        s3_endpoint: Option<String>,
        /// Sign with this profile from ~/.aws/credentials.
        #[arg(long)]
        aws_profile: Option<String>,
    },
    /// Rename this machine as it appears in the history.
    Name { name: String },
    /// Unlock an encrypted ledger on this machine, and keep the key.
    Unlock,
    /// Forget the key on this machine.
    Lock,
    /// Make `ledger` a second name for hl, or stop.
    Alias {
        #[arg(value_parser = ["on", "off", "status"], default_value = "status")]
        state: String,
    },

    /// Net worth, assets, debts, and monthly income and budget.
    Summary,
    /// Accounts, or one account; `hl account balance` sets a balance.
    #[command(alias = "account")]
    Accounts {
        #[command(subcommand)]
        action: Option<AccountAction>,
    },
    /// Savings buckets, or one; `hl bucket add|spend|set|move` moves money.
    #[command(alias = "bucket")]
    Buckets {
        #[command(subcommand)]
        action: Option<BucketAction>,
    },
    /// The monthly budget.
    Budget,
    /// Income streams.
    Income,
    /// Goals and how far along each is.
    Goals,
    /// Holdings and what they are worth.
    Holdings,
    /// Retirement accounts.
    Retirement,
    /// Card statements, or one; also start, import into, settle and undo them.
    #[command(alias = "statement")]
    Statements {
        #[command(subcommand)]
        action: Option<StatementAction>,
    },
    /// What the card statements itemise for a period.
    Spending {
        /// 1m, 3m, 6m, 1y, all, or custom with --from and --to.
        #[arg(long, default_value = "1m")]
        period: String,
        /// all, open or settled.
        #[arg(long, default_value = "all")]
        status: String,
        #[arg(long, default_value = "")]
        from: String,
        #[arg(long, default_value = "")]
        to: String,
    },
    /// Every bucket projected forward.
    Plan {
        /// A date, or a span such as 6m or 1y.
        #[arg(long, default_value = "1y")]
        to: String,
    },
    /// The change history.
    History {
        /// A date, or a span such as 7d or 2m.
        #[arg(long)]
        since: Option<String>,
        /// A machine's name or install id.
        #[arg(long)]
        by: Option<String>,
        /// desktop, cli, mobile or plugin.
        #[arg(long)]
        client: Option<String>,
    },
    /// The whole ledger, or one collection of it, as JSON or CSV.
    Export {
        #[arg(long, default_value = "json")]
        format: String,
        /// accounts, income, budget, buckets, investments, goals, reconciliations…
        collection: Option<String>,
    },

    /// Add one paycheck's worth to every bucket an earner funds.
    Payday {
        earner: String,
        /// Take that payday back out.
        #[arg(long)]
        undo: bool,
    },
    /// Buy or sell shares of a holding.
    Trade {
        #[arg(value_parser = ["buy", "sell"])]
        side: String,
        /// The holding's name or ticker.
        holding: String,
        quantity: String,
        price: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Refresh or set share prices.
    Prices {
        #[command(subcommand)]
        action: PriceAction,
    },
    /// Take today's net worth point, as the app does when it opens.
    Snapshot,
    /// Any edit the app can make, as JSON from a file, or from stdin with `-`.
    Apply { source: String },
    /// Read a ledger file written by another client.
    Import {
        file: String,
        /// Make it this ledger; without this, only show what it holds.
        #[arg(long)]
        replace: bool,
    },
    /// Bank connections through Plaid, with your own keys. Alone, shows them.
    Bank {
        #[command(subcommand)]
        action: Option<BankAction>,
    },
    /// For the Windows installer: put a folder on the user's PATH, or take it
    /// off, keeping every other entry exactly as it was.
    #[command(hide = true)]
    InstallPath {
        #[arg(value_parser = ["add", "remove"])]
        action: String,
        dir: String,
    },
    /// Bring in the Omarchy plugin's change history.
    ImportHistory {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum BankAction {
    /// Save your Plaid keys: the secret from PLAID_SECRET or asked for.
    Keys {
        #[arg(long, value_parser = ["sandbox", "production"], default_value = "sandbox")]
        environment: String,
        /// Your Plaid client id; from PLAID_CLIENT_ID or asked for if left out.
        #[arg(long)]
        client_id: Option<String>,
        /// Remove the saved keys instead.
        #[arg(long)]
        forget: bool,
    },
    /// Connect a bank, in your browser through Plaid.
    Connect {
        /// Sign an existing connection in again, by its bank's name.
        #[arg(long)]
        again: Option<String>,
    },
    /// Link a bank account (name, last four digits or id) to a ledger
    /// account, or to "none".
    Link {
        account: String,
        ledger_account: String,
    },
    /// Fetch new transactions and balances from every connected bank.
    Fetch,
    /// The bank's balances where they differ from the ledger's.
    Balances {
        /// Accept every one of them.
        #[arg(long)]
        apply: bool,
    },
    /// End a connection, by its bank's name.
    Disconnect { bank: String },
}

#[derive(Subcommand)]
enum AccountAction {
    /// One account in full.
    Show { account: String },
    /// Set an account's balance.
    Balance { account: String, amount: String },
}

#[derive(Subcommand)]
enum BucketAction {
    /// One bucket in full.
    Show { bucket: String },
    Add {
        bucket: String,
        amount: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    Spend {
        bucket: String,
        amount: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Set the bucket's cash to match a real account.
    Set { bucket: String, amount: String },
    /// Say which account a bucket's money is kept in, so spending from it
    /// comes out of that account. With --unlinked, every bucket with none.
    Link {
        #[arg(required_unless_present = "unlinked")]
        bucket: Option<String>,
        #[arg(long, conflicts_with = "bucket")]
        unlinked: bool,
        #[arg(long)]
        to: String,
    },
    Move {
        bucket: String,
        amount: String,
        #[arg(long)]
        to: String,
    },
}

#[derive(Subcommand)]
enum StatementAction {
    /// One statement, by id or by card, with its charges.
    Show { statement: String },
    /// Start a statement.
    New {
        #[arg(long)]
        card: String,
        #[arg(long)]
        balance: String,
        #[arg(long)]
        date: String,
        /// The account bucket money comes out of when it is settled.
        /// Taken from the card's last statement when left out.
        #[arg(long)]
        bucket_source: Option<String>,
        /// The account everyday spending comes out of.
        #[arg(long)]
        spend_source: Option<String>,
        /// Settling moves only the buckets and the card, not the accounts.
        #[arg(long)]
        no_account_moves: bool,
    },
    /// Add the purchases from a bank or card CSV export, or with
    /// --from-bank straight from the card's bank connection.
    Import {
        statement: String,
        #[arg(required_unless_present = "from_bank")]
        file: Option<String>,
        /// Fetch the card's charges from its bank instead of a file.
        #[arg(long, conflicts_with = "file")]
        from_bank: bool,
        /// Who spent them, to start with.
        #[arg(long, default_value = "All")]
        member: String,
        /// The bucket they come out of. Left out: everyday spending for a file,
        /// and for the bank, where each merchant's charges went last.
        #[arg(long)]
        bucket: Option<String>,
    },
    /// Settle a statement.
    Settle {
        statement: String,
        /// Where a short bucket's rest comes from: a bucket, everyday or
        /// negative, or Groceries=Overflow for one bucket. May repeat.
        #[arg(long)]
        cover: Vec<String>,
    },
    /// Undo a settled statement.
    Undo { statement: String },
}

#[derive(Subcommand)]
enum PriceAction {
    /// Ask the quote service for every holding with a ticker.
    Refresh,
    /// Price every holding with this ticker by hand.
    Set { ticker: String, price: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // The installer's PATH change needs no ledger, and must not open one.
    if let Command::InstallPath { action, dir } = &cli.command {
        return install_path::run(action, dir);
    }

    // The plugin-history import runs its own runtime, so it starts before ours.
    if let Command::ImportHistory { args } = &cli.command {
        return ExitCode::from(ledger_app::plugin_history::run_cli(args).clamp(0, 255) as u8);
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => return out::exit(Err(Failure::Error(e.to_string()))),
    };
    out::exit(runtime.block_on(run(cli)))
}

async fn run(cli: Cli) -> Outcome {
    let s = Session::open(cli.via, cli.json, cli.dry_run)?;
    match cli.command {
        Command::Status => return setup::status(&s).await,
        Command::Name { name } => return setup::name(&s, &name).await,
        Command::Init {
            name,
            local,
            s3_bucket,
            s3_key,
            s3_region,
            s3_endpoint,
            aws_profile,
        } => {
            return setup::init(
                &s,
                setup::Init {
                    name,
                    local,
                    s3_bucket,
                    s3_key,
                    s3_region,
                    s3_endpoint,
                    aws_profile,
                },
            )
            .await;
        }
        Command::Unlock => return setup::unlock(&s).await,
        Command::Lock => return setup::lock(&s).await,
        Command::Alias { state } => {
            return match state.as_str() {
                "on" => alias::set(&s, true),
                "off" => alias::set(&s, false),
                _ => alias::status(&s),
            };
        }
        _ => s.ready().await?,
    }

    match cli.command {
        Command::Sync => setup::sync(&s).await,
        Command::Summary => read::summary(&s).await,
        Command::Accounts { action } => match action {
            None => read::accounts(&s, None).await,
            Some(AccountAction::Show { account }) => read::accounts(&s, Some(&account)).await,
            Some(AccountAction::Balance { account, amount }) => {
                write::account_balance(&s, &account, &amount).await
            }
        },
        Command::Buckets { action } => match action {
            None => read::buckets(&s, None).await,
            Some(BucketAction::Show { bucket }) => read::buckets(&s, Some(&bucket)).await,
            Some(BucketAction::Add {
                bucket,
                amount,
                note,
            }) => write::bucket(&s, &bucket, write::BucketAction::Add(&amount), &note).await,
            Some(BucketAction::Spend {
                bucket,
                amount,
                note,
            }) => write::bucket(&s, &bucket, write::BucketAction::Spend(&amount), &note).await,
            Some(BucketAction::Set { bucket, amount }) => {
                write::bucket(&s, &bucket, write::BucketAction::Set(&amount), "").await
            }
            Some(BucketAction::Link {
                bucket,
                unlinked: _,
                to,
            }) => write::bucket_link(&s, bucket.as_deref(), &to).await,
            Some(BucketAction::Move { bucket, amount, to }) => {
                write::bucket(
                    &s,
                    &bucket,
                    write::BucketAction::Move {
                        amount: &amount,
                        to: &to,
                    },
                    "",
                )
                .await
            }
        },
        Command::Budget => read::budget(&s).await,
        Command::Income => read::income(&s).await,
        Command::Goals => read::goals(&s).await,
        Command::Holdings => read::holdings(&s).await,
        Command::Retirement => read::retirement(&s).await,
        Command::Statements { action } => match action {
            None => read::statements(&s, None).await,
            Some(StatementAction::Show { statement }) => {
                read::statements(&s, Some(&statement)).await
            }
            Some(StatementAction::New {
                card,
                balance,
                date,
                bucket_source,
                spend_source,
                no_account_moves,
            }) => {
                write::statement_new(
                    &s,
                    write::NewStatement {
                        card: &card,
                        balance: &balance,
                        date: &date,
                        bucket_source: bucket_source.as_deref(),
                        spend_source: spend_source.as_deref(),
                        account_moves: !no_account_moves,
                    },
                )
                .await
            }
            Some(StatementAction::Import {
                statement,
                file,
                from_bank: _,
                member,
                bucket,
            }) => {
                write::statement_import(&s, &statement, file.as_deref(), &member, bucket.as_deref())
                    .await
            }
            Some(StatementAction::Settle { statement, cover }) => {
                write::statement_settle(&s, &statement, &cover).await
            }
            Some(StatementAction::Undo { statement }) => {
                write::statement_undo(&s, &statement).await
            }
        },
        Command::Spending {
            period,
            status,
            from,
            to,
        } => read::spending(&s, &period, &status, &from, &to).await,
        Command::Plan { to } => read::plan(&s, &to).await,
        Command::History { since, by, client } => {
            read::history(&s, since.as_deref(), by.as_deref(), client.as_deref()).await
        }
        Command::Export { format, collection } => {
            read::export(&s, &format, collection.as_deref()).await
        }
        Command::Payday { earner, undo } => write::payday(&s, &earner, undo).await,
        Command::Trade {
            side,
            holding,
            quantity,
            price,
            note,
        } => write::trade(&s, &side, &holding, &quantity, &price, &note).await,
        Command::Prices { action } => match action {
            PriceAction::Refresh => setup::prices_refresh(&s).await,
            PriceAction::Set { ticker, price } => write::price_set(&s, &ticker, &price).await,
        },
        Command::Snapshot => setup::snapshot(&s).await,
        Command::Apply { source } => write::apply(&s, &source).await,
        Command::Import { file, replace } => setup::import(&s, &file, replace).await,
        Command::Bank { action } => match action {
            None => bank::status(&s).await,
            Some(BankAction::Keys {
                environment,
                client_id,
                forget,
            }) => bank::keys(&s, &environment, client_id, forget).await,
            Some(BankAction::Connect { again }) => bank::connect(&s, again.as_deref()).await,
            Some(BankAction::Link {
                account,
                ledger_account,
            }) => bank::link(&s, &account, &ledger_account).await,
            Some(BankAction::Fetch) => bank::fetch(&s).await,
            Some(BankAction::Balances { apply }) => bank::balances(&s, apply).await,
            Some(BankAction::Disconnect { bank: which }) => bank::disconnect(&s, &which).await,
        },
        // Handled before the session opened.
        Command::Status
        | Command::Name { .. }
        | Command::Init { .. }
        | Command::Unlock
        | Command::Lock
        | Command::Alias { .. }
        | Command::InstallPath { .. }
        | Command::ImportHistory { .. } => unreachable!(),
    }
}
