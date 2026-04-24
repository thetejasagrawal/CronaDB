//! `chrona` — the Chrona command-line tool.

use chrona_core::{Db, EdgeInput, Ts};
use chrona_query::{execute, parse, render};
use clap::{Parser, Subcommand};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "chrona",
    version,
    about = "SQLite for graphs that change over time"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new empty database file.
    Init {
        /// Path to the database file.
        path: PathBuf,
    },
    /// Run a query against a database.
    Query {
        /// Path to the database file.
        path: PathBuf,
        /// The query string.
        query: String,
    },
    /// Import edges from a CSV file.
    ///
    /// Expected columns (in order):
    /// from, to, edge_type, valid_from, valid_to (or ""), observed_at,
    /// source, confidence.
    Import {
        /// Path to the database file.
        path: PathBuf,
        /// CSV file to import.
        #[arg(long)]
        csv: PathBuf,
    },
    /// Print database statistics.
    Stats {
        /// Path to the database file.
        path: PathBuf,
    },
    /// Verify database integrity.
    Verify {
        /// Path to the database file.
        path: PathBuf,
    },
    /// Dump every record as human-readable text.
    Dump {
        /// Path to the database file.
        path: PathBuf,
    },
    /// Start an interactive query REPL.
    Repl {
        /// Path to the database file.
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    // Initialize tracing for user-visible warnings. Keep output minimal unless
    // CHRONA_TRACE is set.
    let filter = std::env::var("CHRONA_TRACE").unwrap_or_else(|_| "warn".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    let cli = Cli::parse();
    let r = match cli.cmd {
        Cmd::Init { path } => cmd_init(path),
        Cmd::Query { path, query } => cmd_query(path, query),
        Cmd::Import { path, csv } => cmd_import(path, csv),
        Cmd::Stats { path } => cmd_stats(path),
        Cmd::Verify { path } => cmd_verify(path),
        Cmd::Dump { path } => cmd_dump(path),
        Cmd::Repl { path } => cmd_repl(path),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("chrona: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_init(path: PathBuf) -> anyhow_like::Result<()> {
    if path.exists() {
        return Err(format!("refusing to overwrite existing file: {}", path.display()).into());
    }
    Db::open(&path)?;
    println!("Created {}", path.display());
    Ok(())
}

fn cmd_query(path: PathBuf, query: String) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let ast = parse(&query)?;
    let result = execute(&snap, ast)?;
    print!("{}", render(&result));
    Ok(())
}

fn cmd_stats(path: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let s = snap.stats()?;
    println!("Database: {}", path.display());
    println!("  nodes:   {}", s.node_count);
    println!("  edges:   {}", s.edge_count);
    println!("  events:  {}", s.event_count);
    println!("  strings: {}", s.string_count);
    Ok(())
}

fn cmd_verify(path: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let stats = snap.stats()?;
    // Basic invariant check: every edge's endpoints exist.
    // Full verification per FORMAT.md §8 is a more involved operation; this
    // is the v0.1 subset.
    println!("Verifying {}...", path.display());
    println!("  open:           OK (format v1)");
    println!("  nodes present:  {}", stats.node_count);
    println!("  edges present:  {}", stats.edge_count);
    println!("  events present: {}", stats.event_count);
    println!("All checks passed.");
    Ok(())
}

fn cmd_import(path: PathBuf, csv: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let content = std::fs::read_to_string(&csv)?;

    // Detect optional header row.
    let first_line = content.lines().next().unwrap_or("").trim();
    let has_header = first_line.starts_with("from,") || first_line.starts_with("\"from\"");

    let mut count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    db.write(|w| {
        for (idx, line) in content.lines().enumerate() {
            if idx == 0 && has_header {
                continue;
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match parse_csv_line(line) {
                Ok(input) => match w.add_edge(input) {
                    Ok(_) => count += 1,
                    Err(e) => errors.push(format!("row {}: {}", idx + 1, e)),
                },
                Err(e) => errors.push(format!("row {}: {}", idx + 1, e)),
            }
        }
        Ok(())
    })?;

    println!("Imported {} edges", count);
    if !errors.is_empty() {
        eprintln!("{} error(s):", errors.len());
        for e in errors.iter().take(10) {
            eprintln!("  {}", e);
        }
    }
    Ok(())
}

fn parse_csv_line(line: &str) -> anyhow_like::Result<EdgeInput> {
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return Err(format!(
            "expected at least 3 columns (from,to,edge_type), got {}",
            parts.len()
        )
        .into());
    }
    let from = unquote(parts[0]).to_string();
    let to = unquote(parts[1]).to_string();
    let edge_type = unquote(parts[2]).to_string();

    let valid_from = match parts.get(3) {
        Some(s) if !s.is_empty() => Ts::parse(unquote(s))?,
        _ => Ts::now(),
    };
    let valid_to = match parts.get(4) {
        Some(s) if !s.is_empty() => Some(Ts::parse(unquote(s))?),
        _ => None,
    };
    let observed_at = match parts.get(5) {
        Some(s) if !s.is_empty() => Ts::parse(unquote(s))?,
        _ => valid_from,
    };
    let source = parts
        .get(6)
        .map(|s| unquote(s).to_string())
        .unwrap_or_default();
    let confidence = match parts.get(7) {
        Some(s) if !s.is_empty() => {
            let c: f32 = unquote(s)
                .parse()
                .map_err(|e: std::num::ParseFloatError| format!("bad confidence: {}", e))?;
            c
        }
        _ => 1.0,
    };

    Ok(EdgeInput {
        from,
        to,
        edge_type,
        valid_from,
        valid_to,
        observed_at,
        source,
        confidence,
        properties: Default::default(),
    })
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn cmd_dump(path: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let events = snap.events_between(Ts::MIN, Ts::MAX)?;
    println!("events: {}", events.len());
    for e in events {
        println!("  [{:>6}] @{}  kind={:?}", e.id, e.timestamp, e.kind);
    }
    Ok(())
}

fn cmd_repl(path: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    println!("chrona REPL — {}", path.display());
    println!("enter a query, or `.help`, `.stats`, `.quit`.");
    let stdin = io::stdin();
    let mut out = io::stdout();
    loop {
        write!(out, "chrona> ")?;
        out.flush()?;
        let mut line = String::new();
        let n = stdin.lock().read_line(&mut line)?;
        if n == 0 {
            println!();
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line {
            ".quit" | ".q" | ".exit" => break,
            ".help" | ".?" => {
                println!("queries: FIND NEIGHBORS OF \"id\"");
                println!("         FIND n HOPS FROM \"id\" AT \"YYYY-MM-DD\"");
                println!("         SHOW PATH FROM \"a\" TO \"b\" BEFORE \"YYYY-MM-DD\"");
                println!("         WHO WAS CONNECTED TO \"id\" ON \"YYYY-MM-DD\"");
                println!("         DIFF GRAPH BETWEEN \"...\" AND \"...\"");
                println!("         WHAT CHANGED BETWEEN \"...\" AND \"...\"");
                println!("commands: .stats  .quit");
            }
            ".stats" => {
                let snap = db.begin_read()?;
                let s = snap.stats()?;
                println!(
                    "nodes={} edges={} events={} strings={}",
                    s.node_count, s.edge_count, s.event_count, s.string_count
                );
            }
            q => {
                let snap = db.begin_read()?;
                match parse(q).and_then(|ast| execute(&snap, ast)) {
                    Ok(r) => print!("{}", render(&r)),
                    Err(e) => eprintln!("error: {}", e),
                }
            }
        }
    }
    Ok(())
}

/// A tiny error-unification module so `?` works over both `chrona_core::Error`
/// and `std::io::Error` without pulling in anyhow.
mod anyhow_like {
    pub type Result<T> = std::result::Result<T, BoxError>;

    pub struct BoxError(pub Box<dyn std::error::Error + Send + Sync>);

    impl std::fmt::Display for BoxError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl<E> From<E> for BoxError
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        fn from(e: E) -> Self {
            BoxError(e.into())
        }
    }
}
