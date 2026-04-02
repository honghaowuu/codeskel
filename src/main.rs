mod cli;
mod models;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Scan(args) => {
            eprintln!("scan: {:?}", args.project_root);
        }
        Commands::Get(args) => {
            eprintln!("get: {:?}", args.cache_path);
        }
        Commands::Rescan(args) => {
            eprintln!("rescan: {:?}", args.cache_path);
        }
    }
}
