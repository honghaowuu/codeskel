use clap::Parser;
use codeskel::cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Scan(args) => codeskel::commands::scan::run(args),
        Commands::Get(args) => codeskel::commands::get::run(args),
        Commands::Rescan(args) => codeskel::commands::rescan::run(args),
        Commands::Pom(args) => codeskel::commands::pom::run(args),
        Commands::Next(args) => codeskel::commands::next::run(args),
    };
    match result {
        Ok(had_warnings) => {
            if had_warnings {
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
