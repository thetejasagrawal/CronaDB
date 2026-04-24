//! `chrona` — the Chrona command-line tool.

use chrona_core::{Db, EdgeInput, PropValue, Props, Ts};
use chrona_query::{execute, parse, render, render_json};
use clap::{Parser, Subcommand, ValueEnum};
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
        /// Emit results as single-line JSON.
        #[arg(long)]
        json: bool,
    },
    /// Import edges from a file. Format is autodetected from the extension.
    ///
    /// CSV columns (in order): from, to, edge_type, valid_from, valid_to,
    /// observed_at, source, confidence.
    ///
    /// JSONL (one object per line): keys `from`, `to`, `edge_type`,
    /// `valid_from`, `valid_to`, `observed_at`, `source`, `confidence`,
    /// `properties`.
    Import {
        /// Path to the database file.
        path: PathBuf,
        /// Source file (CSV or JSONL).
        #[arg(long)]
        file: PathBuf,
        /// Force a specific format instead of detecting by extension.
        #[arg(long, value_enum)]
        format: Option<ImportFmt>,
    },
    /// Print database statistics.
    Stats {
        /// Path to the database file.
        path: PathBuf,
        /// Emit stats as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Verify database integrity against FORMAT.md §8.
    Verify {
        /// Path to the database file.
        path: PathBuf,
    },
    /// Alias for verify.
    Fsck {
        /// Path to the database file.
        path: PathBuf,
    },
    /// List every node.
    Nodes {
        /// Path to the database file.
        path: PathBuf,
        /// Emit as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List every edge.
    Edges {
        /// Path to the database file.
        path: PathBuf,
        /// Emit as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Walk an edge's revision chain backwards to the original observation.
    History {
        /// Path to the database file.
        path: PathBuf,
        /// Starting edge id (numeric).
        edge_id: u64,
    },
    /// Dump every event as human-readable text.
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

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ImportFmt {
    Csv,
    Jsonl,
}

fn main() -> ExitCode {
    let filter = std::env::var("CHRONA_TRACE").unwrap_or_else(|_| "warn".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    let cli = Cli::parse();
    let r = match cli.cmd {
        Cmd::Init { path } => cmd_init(path),
        Cmd::Query { path, query, json } => cmd_query(path, query, json),
        Cmd::Import { path, file, format } => cmd_import(path, file, format),
        Cmd::Stats { path, json } => cmd_stats(path, json),
        Cmd::Verify { path } | Cmd::Fsck { path } => cmd_verify(path),
        Cmd::Nodes { path, json } => cmd_nodes(path, json),
        Cmd::Edges { path, json } => cmd_edges(path, json),
        Cmd::History { path, edge_id } => cmd_history(path, edge_id),
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

fn cmd_query(path: PathBuf, query: String, json: bool) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let ast = parse(&query)?;
    let result = execute(&snap, ast)?;
    if json {
        print!("{}", render_json(&result));
    } else {
        print!("{}", render(&result));
    }
    Ok(())
}

fn cmd_stats(path: PathBuf, json: bool) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let s = snap.stats()?;
    if json {
        println!(
            "{{\"path\":\"{}\",\"nodes\":{},\"edges\":{},\"events\":{},\"strings\":{}}}",
            path.display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\""),
            s.node_count,
            s.edge_count,
            s.event_count,
            s.string_count
        );
    } else {
        println!("Database: {}", path.display());
        println!("  nodes:   {}", s.node_count);
        println!("  edges:   {}", s.edge_count);
        println!("  events:  {}", s.event_count);
        println!("  strings: {}", s.string_count);
    }
    Ok(())
}

fn cmd_verify(path: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let report = snap.verify()?;
    println!("Verifying {}...", path.display());
    for line in &report.lines {
        println!("  {}", line);
    }
    if report.errors.is_empty() {
        println!("All checks passed.");
        Ok(())
    } else {
        for e in &report.errors {
            eprintln!("  [FAIL] {}", e);
        }
        Err(format!("{} error(s) found", report.errors.len()).into())
    }
}

fn cmd_nodes(path: PathBuf, json: bool) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let nodes = snap.all_nodes()?;
    if json {
        print!("[");
        let mut first = true;
        for n in &nodes {
            if !first {
                print!(",");
            }
            first = false;
            let type_name = match n.type_id {
                Some(id) => snap.resolve_string(id).ok(),
                None => None,
            };
            print!(
                "{{\"id\":{},\"ext_id\":{:?},\"type\":{},\"created_at\":{:?}}}",
                n.id.raw(),
                n.ext_id,
                match type_name {
                    Some(t) => format!("{:?}", t),
                    None => "null".to_string(),
                },
                n.created_at.to_rfc3339()
            );
        }
        println!("]");
    } else {
        for n in &nodes {
            let type_name = match n.type_id {
                Some(id) => snap.resolve_string(id).ok(),
                None => None,
            };
            println!(
                "{:<6} {:<20} type={:<15} created={}",
                n.id.to_string(),
                n.ext_id,
                type_name.as_deref().unwrap_or("-"),
                n.created_at
            );
        }
    }
    Ok(())
}

fn cmd_edges(path: PathBuf, json: bool) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let views = snap.all_edges_view()?;
    if json {
        print!("[");
        let mut first = true;
        for v in &views {
            if !first {
                print!(",");
            }
            first = false;
            print!(
                "{{\"id\":{},\"from\":{:?},\"to\":{:?},\"type\":{:?},\"valid_from\":{:?},\
                 \"valid_to\":{},\"source\":{:?},\"confidence\":{:.4}}}",
                v.id.raw(),
                v.from_ext_id,
                v.to_ext_id,
                v.edge_type,
                v.valid_from.to_rfc3339(),
                match v.valid_to {
                    Some(t) => format!("{:?}", t.to_rfc3339()),
                    None => "null".into(),
                },
                v.source,
                v.confidence
            );
        }
        println!("]");
    } else {
        for v in &views {
            println!(
                "{:<6} {:<12} -[{}]-> {:<12} valid=[{}..{}) src={} conf={:.2}",
                v.id.to_string(),
                v.from_ext_id,
                v.edge_type,
                v.to_ext_id,
                v.valid_from,
                v.valid_to
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "open".into()),
                if v.source.is_empty() { "-" } else { &v.source },
                v.confidence
            );
        }
    }
    Ok(())
}

fn cmd_history(path: PathBuf, edge_id: u64) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let snap = db.begin_read()?;
    let start = chrona_core::EdgeId::from_raw(edge_id);
    let chain = snap.revision_chain(start)?;
    if chain.is_empty() {
        return Err(format!("edge {} not found", edge_id).into());
    }
    for (i, e) in chain.iter().enumerate() {
        let view = snap.view_edge(e)?;
        println!(
            "{}[{}] {:<10} -[{}]-> {:<10}  valid_from={}  src={}  conf={:.2}",
            if i == 0 { "→ " } else { "  " },
            e.id,
            view.from_ext_id,
            view.edge_type,
            view.to_ext_id,
            view.valid_from,
            if view.source.is_empty() {
                "-"
            } else {
                &view.source
            },
            view.confidence,
        );
    }
    Ok(())
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
                println!("filters: ... WHERE type = \"X\" AND confidence >= 0.8");
                println!("limits:  ... LIMIT n");
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

// ---- Import ----

fn cmd_import(path: PathBuf, file: PathBuf, format: Option<ImportFmt>) -> anyhow_like::Result<()> {
    let detected = format.unwrap_or_else(|| detect_format(&file));
    match detected {
        ImportFmt::Csv => import_csv(path, file),
        ImportFmt::Jsonl => import_jsonl(path, file),
    }
}

fn detect_format(file: &std::path::Path) -> ImportFmt {
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jsonl" | "ndjson" | "json" => ImportFmt::Jsonl,
        _ => ImportFmt::Csv,
    }
}

fn import_csv(path: PathBuf, csv: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let content = std::fs::read_to_string(&csv)?;

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

    println!("Imported {} edges (csv)", count);
    report_errors(&errors);
    Ok(())
}

fn import_jsonl(path: PathBuf, file: PathBuf) -> anyhow_like::Result<()> {
    let db = Db::open(&path)?;
    let content = std::fs::read_to_string(&file)?;

    let mut count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    db.write(|w| {
        for (idx, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match parse_jsonl_line(line) {
                Ok(input) => match w.add_edge(input) {
                    Ok(_) => count += 1,
                    Err(e) => errors.push(format!("line {}: {}", idx + 1, e)),
                },
                Err(e) => errors.push(format!("line {}: {}", idx + 1, e)),
            }
        }
        Ok(())
    })?;

    println!("Imported {} edges (jsonl)", count);
    report_errors(&errors);
    Ok(())
}

fn report_errors(errors: &[String]) {
    if !errors.is_empty() {
        eprintln!("{} error(s):", errors.len());
        for e in errors.iter().take(10) {
            eprintln!("  {}", e);
        }
    }
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
        Some(s) if !s.is_empty() => unquote(s)
            .parse::<f32>()
            .map_err(|e| format!("bad confidence: {}", e))?,
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

fn parse_jsonl_line(line: &str) -> anyhow_like::Result<EdgeInput> {
    // We keep dependencies minimal — this is a small hand-written JSON parser
    // for flat objects. It does not handle nested structures in non-`properties`
    // fields and accepts only the exact shape described in the Import docs.
    let mut p = json::Parser::new(line);
    let obj = p.parse_object()?;

    let from = obj.get_string("from").ok_or("missing 'from'")?;
    let to = obj.get_string("to").ok_or("missing 'to'")?;
    let edge_type = obj.get_string("edge_type").ok_or("missing 'edge_type'")?;

    let valid_from = match obj.get_string("valid_from") {
        Some(s) if !s.is_empty() => Ts::parse(&s)?,
        _ => Ts::now(),
    };
    let valid_to = match obj.get_string("valid_to") {
        Some(s) if !s.is_empty() => Some(Ts::parse(&s)?),
        _ => None,
    };
    let observed_at = match obj.get_string("observed_at") {
        Some(s) if !s.is_empty() => Ts::parse(&s)?,
        _ => valid_from,
    };
    let source = obj.get_string("source").unwrap_or_default();
    let confidence = obj
        .get_number("confidence")
        .map(|v| v as f32)
        .unwrap_or(1.0);

    let mut properties = Props::new();
    if let Some(obj_props) = obj.get_object("properties") {
        for (k, v) in obj_props.entries() {
            let pv = match v {
                json::Value::Null => PropValue::Null,
                json::Value::Bool(b) => PropValue::Bool(*b),
                json::Value::Number(n) => {
                    if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                        PropValue::Int(*n as i64)
                    } else {
                        PropValue::Float(*n)
                    }
                }
                json::Value::String(s) => PropValue::String(s.clone()),
                json::Value::Object(_) => continue, // skip nested
            };
            properties.insert(k.clone(), pv);
        }
    }

    Ok(EdgeInput {
        from: from.to_string(),
        to: to.to_string(),
        edge_type: edge_type.to_string(),
        valid_from,
        valid_to,
        observed_at,
        source,
        confidence,
        properties,
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

// ---- Tiny JSON parser (for JSONL import only) ----

mod json {
    use super::anyhow_like::BoxError;

    #[derive(Debug, Clone)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Object(Object),
    }

    #[derive(Debug, Clone, Default)]
    pub struct Object {
        pub fields: Vec<(String, Value)>,
    }

    impl Object {
        pub fn get(&self, key: &str) -> Option<&Value> {
            self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        }
        pub fn get_string(&self, key: &str) -> Option<String> {
            match self.get(key) {
                Some(Value::String(s)) => Some(s.clone()),
                _ => None,
            }
        }
        pub fn get_number(&self, key: &str) -> Option<f64> {
            match self.get(key) {
                Some(Value::Number(n)) => Some(*n),
                _ => None,
            }
        }
        pub fn get_object(&self, key: &str) -> Option<&Object> {
            match self.get(key) {
                Some(Value::Object(o)) => Some(o),
                _ => None,
            }
        }
        pub fn entries(&self) -> impl Iterator<Item = (&String, &Value)> {
            self.fields.iter().map(|(k, v)| (k, v))
        }
    }

    pub struct Parser<'a> {
        s: &'a str,
        pos: usize,
    }

    impl<'a> Parser<'a> {
        pub fn new(s: &'a str) -> Self {
            Self { s, pos: 0 }
        }

        fn peek(&self) -> Option<u8> {
            self.s.as_bytes().get(self.pos).copied()
        }

        fn bump(&mut self) -> Option<u8> {
            let c = self.peek()?;
            self.pos += 1;
            Some(c)
        }

        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(c) if c.is_ascii_whitespace()) {
                self.pos += 1;
            }
        }

        fn expect(&mut self, c: u8) -> Result<(), BoxError> {
            self.skip_ws();
            match self.bump() {
                Some(x) if x == c => Ok(()),
                Some(x) => Err(format!("expected {:?}, got {:?}", c as char, x as char).into()),
                None => Err(format!("expected {:?}, got end", c as char).into()),
            }
        }

        pub fn parse_object(&mut self) -> Result<Object, BoxError> {
            self.skip_ws();
            self.expect(b'{')?;
            let mut fields = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.bump();
                return Ok(Object { fields });
            }
            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.skip_ws();
                self.expect(b':')?;
                let value = self.parse_value()?;
                fields.push((key, value));
                self.skip_ws();
                match self.bump() {
                    Some(b',') => continue,
                    Some(b'}') => break,
                    Some(c) => return Err(format!("unexpected {:?}", c as char).into()),
                    None => return Err("unterminated object".into()),
                }
            }
            Ok(Object { fields })
        }

        fn parse_value(&mut self) -> Result<Value, BoxError> {
            self.skip_ws();
            match self.peek() {
                Some(b'"') => Ok(Value::String(self.parse_string()?)),
                Some(b'{') => Ok(Value::Object(self.parse_object()?)),
                Some(b't') | Some(b'f') => self.parse_bool(),
                Some(b'n') => self.parse_null(),
                Some(c) if c.is_ascii_digit() || c == b'-' => self.parse_number(),
                Some(c) => Err(format!("unexpected {:?}", c as char).into()),
                None => Err("unexpected end".into()),
            }
        }

        fn parse_string(&mut self) -> Result<String, BoxError> {
            self.expect(b'"')?;
            let mut out = String::new();
            loop {
                match self.bump() {
                    Some(b'"') => return Ok(out),
                    Some(b'\\') => match self.bump() {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            let mut hex = [0u8; 4];
                            for h in &mut hex {
                                *h = self.bump().ok_or("short unicode escape")?;
                            }
                            let s = std::str::from_utf8(&hex).map_err(|e| e.to_string())?;
                            let code = u32::from_str_radix(s, 16).map_err(|e| e.to_string())?;
                            if let Some(c) = char::from_u32(code) {
                                out.push(c);
                            }
                        }
                        Some(c) => return Err(format!("bad escape \\{}", c as char).into()),
                        None => return Err("short escape".into()),
                    },
                    Some(c) => out.push(c as char),
                    None => return Err("unterminated string".into()),
                }
            }
        }

        fn parse_number(&mut self) -> Result<Value, BoxError> {
            let start = self.pos;
            if self.peek() == Some(b'-') {
                self.bump();
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-')
            {
                self.bump();
            }
            let slice = &self.s[start..self.pos];
            let n: f64 = slice.parse().map_err(|_| "bad number")?;
            Ok(Value::Number(n))
        }

        fn parse_bool(&mut self) -> Result<Value, BoxError> {
            if self.s[self.pos..].starts_with("true") {
                self.pos += 4;
                Ok(Value::Bool(true))
            } else if self.s[self.pos..].starts_with("false") {
                self.pos += 5;
                Ok(Value::Bool(false))
            } else {
                Err("bad boolean".into())
            }
        }

        fn parse_null(&mut self) -> Result<Value, BoxError> {
            if self.s[self.pos..].starts_with("null") {
                self.pos += 4;
                Ok(Value::Null)
            } else {
                Err("expected null".into())
            }
        }
    }
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
