use clap::Parser;
use codeskel::cli::{Cli, Commands};
use codeskel::envelope;
use codeskel::error::CodeskelError;

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
            if let Some(ce) = e.downcast_ref::<CodeskelError>() {
                envelope::print_err_coded(ce.code(), &ce.to_string(), ce.hint())
            }
            envelope::print_err(&format!("{e:#}"), None)
        }
    }
}
