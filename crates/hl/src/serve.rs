//! `hl serve`: hl over HTTP, for a phone on the same tailnet.
//!
//! It listens on this machine alone and is reached through `tailscale serve`,
//! which puts it on the tailnet over HTTPS and names the person asking in
//! `Tailscale-User-Login`. Only the logins given with `--allow` are answered.
//! A request without the `X-Home-Ledger` header is refused too: a browser only
//! sends a custom header after asking first, which this never answers, so a
//! page on another site cannot run a command from the phone's browser.
//!
//! Each command runs as its own `hl`, exactly as typed in a terminal, so its
//! figures, rules and history are the command line's own. Setting up the
//! machine, keys and anything that reads a file here are refused: they are
//! for a terminal on the machine itself.

use std::process::{Command, ExitCode, Stdio};

const PAGE: &str = include_str!("serve.html");

/// Top-level commands that only make sense at the machine's own terminal.
const REFUSED: &[&str] = &[
    "init",
    "name",
    "unlock",
    "lock",
    "alias",
    "install-path",
    "import",
    "import-history",
    "apply",
    "serve",
];

/// `hl bank` actions that ask for secrets or open a browser.
const REFUSED_BANK: &[&str] = &["keys", "connect", "disconnect"];

pub fn run(port: u16, allow: &[String]) -> ExitCode {
    let server = match tiny_http::Server::http(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("hl serve: could not listen on 127.0.0.1:{port}: {e}");
            return ExitCode::from(1);
        }
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("hl serve: {e}");
            return ExitCode::from(1);
        }
    };
    eprintln!(
        "hl serve: on 127.0.0.1:{port} for {}; put it on the tailnet with `tailscale serve --bg {port}`",
        allow.join(", ")
    );

    for mut request in server.incoming_requests() {
        let answer = answer(&mut request, &exe, allow);
        let _ = request.respond(answer);
    }
    ExitCode::SUCCESS
}

type Reply = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

fn answer(request: &mut tiny_http::Request, exe: &std::path::Path, allow: &[String]) -> Reply {
    use tiny_http::Method;
    let path = request.url().split('?').next().unwrap_or("").to_string();
    match (request.method(), path.as_str()) {
        (Method::Get, "/healthz") => text(200, "ok"),
        (Method::Get, "/") => {
            if let Err(r) = who(request, allow) {
                return r;
            }
            reply(200, PAGE, "text/html; charset=utf-8")
        }
        (Method::Post, "/api/run") => {
            if let Err(r) = who(request, allow) {
                return r;
            }
            if header(request, "X-Home-Ledger").is_none() {
                return text(403, "missing the X-Home-Ledger header");
            }
            let mut body = String::new();
            if std::io::Read::read_to_string(&mut request.as_reader(), &mut body).is_err() {
                return text(400, "the command must be text");
            }
            let json = header(request, "Accept").is_some_and(|a| a.contains("application/json"));
            let args = match words(&body).and_then(|a| allowed(a).map_err(String::from)) {
                Ok(a) => a,
                Err(e) if json => {
                    return reply(400, &ran(2, "", &e), "application/json");
                }
                Err(e) => return text(400, &e),
            };
            let (code, out, err) = hl(exe, &args);
            if json {
                return reply(200, &ran(code, &out, &err), "application/json");
            }
            let mut r = text(200, &format!("{out}{err}"));
            if let Ok(h) = tiny_http::Header::from_bytes("X-Exit-Code", code.to_string()) {
                r.add_header(h);
            }
            r
        }
        _ => text(404, "not found"),
    }
}

/// The tailnet login asking, when it is one of the allowed ones.
fn who(request: &tiny_http::Request, allow: &[String]) -> Result<(), Reply> {
    match header(request, "Tailscale-User-Login") {
        Some(login) if allow.iter().any(|a| a.eq_ignore_ascii_case(login.trim())) => Ok(()),
        Some(_) => Err(text(403, "this login is not allowed")),
        None => Err(text(403, "reach this through `tailscale serve`")),
    }
}

fn header<'a>(request: &'a tiny_http::Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

/// Runs one command as its own hl, labelled in the history as from the phone.
fn hl(exe: &std::path::Path, args: &[String]) -> (i32, String, String) {
    let mut command = Command::new(exe);
    if !args.iter().any(|a| a == "--via" || a.starts_with("--via=")) {
        command.args(["--via", "phone"]);
    }
    match command
        .args(args)
        .stdin(Stdio::null())
        .env("NO_COLOR", "1")
        .output()
    {
        Ok(o) => (
            o.status.code().unwrap_or(1),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        ),
        Err(e) => (1, String::new(), format!("could not run hl: {e}\n")),
    }
}

fn ran(code: i32, out: &str, err: &str) -> String {
    serde_json::json!({ "exit": code, "out": out, "err": err }).to_string()
}

fn reply(status: u16, body: &str, kind: &str) -> Reply {
    let mut r = tiny_http::Response::from_data(body.as_bytes().to_vec()).with_status_code(status);
    if let Ok(h) = tiny_http::Header::from_bytes("Content-Type", kind) {
        r.add_header(h);
    }
    if let Ok(h) = tiny_http::Header::from_bytes("Cache-Control", "no-store") {
        r.add_header(h);
    }
    r
}

fn text(status: u16, body: &str) -> Reply {
    reply(status, body, "text/plain; charset=utf-8")
}

/// A command line split as a shell would: on spaces, with quotes keeping a
/// phrase together. A leading `hl` is dropped, so either form can be typed.
pub fn words(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.trim().chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => match chars.next() {
                Some(next) => word.push(next),
                None => return Err("the command ends in a lone \\".into()),
            },
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("a quote is never closed".into());
    }
    if started {
        words.push(word);
    }
    if words.first().is_some_and(|w| w == "hl" || w == "ledger") {
        words.remove(0);
    }
    if words.is_empty() {
        return Err("type a command, such as `summary`".into());
    }
    Ok(words)
}

/// The words, unless they name a command that is refused over the network.
pub fn allowed(words: Vec<String>) -> Result<Vec<String>, &'static str> {
    // The global options can come first; --via takes a value.
    let mut positional = Vec::new();
    let mut rest = words.iter();
    while let Some(w) = rest.next() {
        if w == "--via" {
            rest.next();
        } else if !w.starts_with('-') {
            positional.push(w.as_str());
        }
    }
    match positional.as_slice() {
        [first, ..] if REFUSED.contains(first) => {
            Err("that is for a terminal on the machine itself")
        }
        ["bank", action, ..] if REFUSED_BANK.contains(action) => {
            Err("that is for a terminal on the machine itself")
        }
        _ => Ok(words),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(line: &str) -> Vec<String> {
        words(line).unwrap()
    }

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(w("buckets"), ["buckets"]);
        assert_eq!(
            w("  hl bucket spend Groceries 52.10  "),
            ["bucket", "spend", "Groceries", "52.10"]
        );
        assert_eq!(
            w(r#"bucket spend "Kids Fun" 20 --note 'zoo day'"#),
            ["bucket", "spend", "Kids Fun", "20", "--note", "zoo day"]
        );
        assert_eq!(w(r#"bucket add X 1 --note "say \"hi\"""#)[5], r#"say "hi""#);
        assert_eq!(w(r#"bucket add X 1 --note """#)[5], "");
    }

    #[test]
    fn refuses_what_it_cannot_split() {
        assert!(words("").is_err());
        assert!(words("hl").is_err());
        assert!(words("bucket add 'X 1").is_err());
    }

    #[test]
    fn refuses_setup_and_secrets() {
        for line in [
            "init --name x --local /tmp/l.json",
            "unlock",
            "--json lock",
            "--via x alias on",
            "import ledger.json --replace",
            "apply -",
            "bank keys",
            "bank connect",
            "--json bank disconnect Chase",
            "serve --allow me",
        ] {
            assert!(allowed(w(line)).is_err(), "{line}");
        }
    }

    #[test]
    fn passes_everything_else() {
        for line in [
            "summary",
            "--json buckets",
            "--via import buckets",
            "bucket spend Groceries 10",
            "statement settle Sapphire",
            "bank",
            "bank fetch",
            "payday Alex --dry-run",
            "help",
        ] {
            assert!(allowed(w(line)).is_ok(), "{line}");
        }
    }
}
