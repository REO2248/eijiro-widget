use anyhow::Result;
use clap::{Parser, Subcommand};
use log::info;
use std::path::PathBuf;
use eijiro_widget::{EijiroParser, IndexBuilder, IndexPaths, SearchResult, PrefixSearchEngine, FullTextSearchEngine};

mod gtk_ui;

const DEFAULT_UI_LIMIT: usize = 100;

#[derive(Parser)]
#[command(name = "eijiro-widget")]
#[command(about = "Fast Eijiro search tool with GTK UI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short, long, global = true, default_value = "info")]
    log_level: String,

    #[arg(short, long, global = true)]
    index_dir: Option<PathBuf>,

    #[arg(short = 'n', long, global = true, default_value_t = DEFAULT_UI_LIMIT)]
    limit: usize,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        #[arg(value_name = "FILE")]
        eijiro_file: PathBuf,

        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,
    },

    Prefix {
        query: String,
    },

    Fulltext {
        query: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&cli.log_level))
        .init();

    match cli.command {
        None => {
            let index_dir = cli.index_dir.unwrap_or_else(default_index_dir);
            gtk_ui::run_gtk_ui(index_dir, cli.limit)?;
        }
        Some(Commands::Build { eijiro_file, output }) => {
            let output_dir = output.or(cli.index_dir);
            build_index(&eijiro_file, output_dir.as_deref())?;
        }
        Some(Commands::Prefix { query }) => {
            let index_dir = cli.index_dir.unwrap_or_else(default_index_dir);
            let paths = IndexPaths::new(&index_dir);
            let engine = PrefixSearchEngine::load(&paths)?;
            let result = engine.search_prefix(&query, cli.limit)?;
            print_result(&result);
        }
        Some(Commands::Fulltext { query }) => {
            let index_dir = cli.index_dir.unwrap_or_else(default_index_dir);
            let paths = IndexPaths::new(&index_dir);
            let engine = FullTextSearchEngine::load(&paths)?;
            let result = engine.search_fulltext(&query, cli.limit)?;
            print_result(&result);
        }
    }

    Ok(())
}

fn build_index(eijiro_file: &std::path::Path, output: Option<&std::path::Path>) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| std::path::Path::new(".eijiro"));

    info!("Parsing Eijiro file: {}", eijiro_file.display());
    let entries = EijiroParser::parse_file(eijiro_file)?;
    info!("Loaded {} entries.", entries.len());

    std::fs::create_dir_all(output_dir)?;
    let builder = IndexBuilder::new(output_dir);
    builder.build(&entries)?;

    info!("Index build complete.");
    Ok(())
}

fn default_index_dir() -> PathBuf {
    let local_path = PathBuf::from(".eijiro");
    if local_path.exists() && local_path.is_dir() {
        return local_path;
    }

    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join(".eijiro")
}

fn print_result(result: &SearchResult) {
    println!("Query: \"{}\" ({:?})", result.query, result.search_type);
    println!("Found {} results:", result.entries.len());
    println!("----------------------------------------");
    for (i, entry) in result.entries.iter().enumerate() {
        println!("{}. {}", i + 1, entry.headword);
        for sense in &entry.senses {
            let attrs = sense
                .attributes
                .iter()
                .map(|a| format!("{a:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            if attrs.is_empty() {
                println!("   - {}", sense.description);
            } else {
                println!("   - [{attrs}] {}", sense.description);
            }
        }
        println!();
    }
}
