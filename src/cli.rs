use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "codeskel", about = "Project dependency & coverage scanner")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze project and write cache
    Scan(ScanArgs),
    /// Query individual file from cache
    Get(GetArgs),
    /// Re-analyze specific files after commenting
    Rescan(RescanArgs),
    /// Extract Maven project metadata from pom.xml
    Pom(PomArgs),
    /// Rescan the last-returned file and return the next file + its dep signatures
    Next(NextArgs),
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Path to the project root
    pub project_root: std::path::PathBuf,

    /// Force language (java|python|ts|js|go|rust|cs|cpp|ruby)
    #[arg(short, long)]
    pub lang: Option<String>,

    /// Only include files matching glob (repeatable)
    #[arg(long)]
    pub include: Vec<String>,

    /// Exclude files matching glob (repeatable)
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Skip files with existing comment coverage above this threshold
    #[arg(long, default_value = "0.8")]
    pub min_coverage: f64,

    /// Minimum prose word count to consider a docstring adequate (0 = presence only)
    #[arg(long, default_value = "10")]
    pub min_docstring_words: usize,

    /// Where to write cache
    #[arg(long)]
    pub cache_dir: Option<std::path::PathBuf>,

    /// Print progress to stderr
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Path to .codeskel/cache.json
    pub cache_path: std::path::PathBuf,

    /// Return file at position N in the order array (0-based)
    #[arg(long)]
    pub index: Option<usize>,

    /// Return file by path (relative to project root)
    #[arg(long)]
    pub path: Option<String>,

    /// Return signature summaries of direct dependencies of FILE (relative to project root)
    #[arg(long, value_name = "FILE")]
    pub deps: Option<String>,

    /// Return transitive dep chain count, or with --index fetch one dep entry
    #[arg(long, value_name = "FILE")]
    pub chain: Option<String>,

    /// Return symbol references from FILE's body to its internal deps (Java only)
    #[arg(long, value_name = "FILE")]
    pub refs: Option<String>,
}

#[derive(Args, Debug)]
pub struct RescanArgs {
    /// Path to .codeskel/cache.json
    pub cache_path: std::path::PathBuf,

    /// Files to re-parse
    pub file_paths: Vec<std::path::PathBuf>,
}

#[derive(Args, Debug)]
pub struct NextArgs {
    /// Path to .codeskel/cache.json (default: .codeskel/cache.json)
    #[arg(long, default_value = ".codeskel/cache.json")]
    pub cache: std::path::PathBuf,

    /// Restrict loop to the transitive dep chain of this file (relative path)
    #[arg(long)]
    pub target: Option<String>,
}

#[derive(Args, Debug)]
pub struct PomArgs {
    /// Path to the project root (default: current directory)
    #[arg(default_value = ".")]
    pub project_root: std::path::PathBuf,

    /// Path hint for multi-module resolution
    #[arg(long)]
    pub controller_path: Option<String>,
}
