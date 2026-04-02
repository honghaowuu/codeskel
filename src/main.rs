use clap::Parser;
use codeskel::cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Scan(args) => codeskel::commands::scan::run(args),
        Commands::Get(args) => codeskel::commands::get::run(args),
        Commands::Rescan(args) => codeskel::commands::rescan::run(args),
    };
    match result {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
