// Windows release builds must not open a console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("import-plugin-history") {
        std::process::exit(home_ledger_lib::import_plugin_history(&args[1..]));
    }
    home_ledger_lib::run()
}
