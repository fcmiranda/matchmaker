mod action;
mod clap;
mod color;
mod config;
mod crokey;
mod fm;
pub mod formatter;
mod logger;
mod parse;
mod paths;
mod register;
mod start;
mod utils;

use clap::*;
use config::PartialConfig;
use logger::*;
use paths::*;
use start::*;
use utils::*;

use std::process::exit;

use cba::{bait::ResultExt, bog::BogOkExt, bring::split::split_on_unescaped_delimiter, ebog};

use matchmaker::MatchError;
use matchmaker_partial::Set;

use crate::parse::{get_pairs, try_split_kv};

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let (cli, config_args) = Cli::get_partitioned_args();

    init_logger(
        [cli.quiet, cli.verbose],
        &state_dir().join(format!("{BINARY_SHORT}.log")),
    );
    log::debug!("{cli:?}, {config_args:?}");

    display_doc(&cli);
    handle_download(&cli);

    if handle_frecency_cli(&config_args) {
        exit(0);
    }

    // get config overrides
    let partial = get_partial(config_args).__ebog();
    log::trace!("{partial:?}");

    let no_read = cli.no_read;
    let group_prefix = cli.group_prefix.clone();
    // get config
    let config = enter(cli, partial).__ebog();

    // begin
    match start(config, no_read, group_prefix).await {
        Ok(_) => {
            log::debug!("Execution Complete");
        }
        Err(err) => match err {
            MatchError::Abort(i) => {
                exit(i);
            }
            MatchError::EventLoopClosed => {
                exit(127);
            }
            MatchError::TUIError(e) => {
                ebog!("TUI"; "{e}")
            }
            MatchError::NoMatch => {
                ebog!("NoMatch");
                exit(404);
            }
            _ => unreachable!(),
        },
    };
}

fn get_partial(config_args: Vec<String>) -> anyhow::Result<PartialConfig> {
    let split = get_pairs(config_args)?;
    log::trace!("{split:?}");
    let mut partial = PartialConfig::default();
    for (path, val) in split {
        if !path.is_empty() && (path[0] == "env" || path[0] == "envs") {
            cba::wbog!(
                "Ignoring manual override of environment variables via CLI: {}",
                path.join(".")
            );
            continue;
        }

        let parts = {
            let mut parts = split_on_unescaped_delimiter(&val, "|||");
            let is_binds = path.len() == 1 && ["binds", "b"].contains(&path[0].as_ref());
            try_split_kv(&mut parts, is_binds)?;
            parts
        };


        partial
            .set(path.as_slice(), &parts)
            .prefix(format!("Invalid value for {}", path.join(".")))?;
    }

    Ok(partial)
}

fn display_doc(cli: &Cli) {
    use termimad::MadSkin;
    use termimad::crossterm::style::Color;

    let mut md = String::new();
    if let Some(doc) = &cli.doc {
        match doc {
            Doc::Options => md.push_str(include_str!("../assets/docs/options.md")),
            Doc::Binds => md.push_str(include_str!("../assets/docs/binds.md")),
            Doc::Template => md.push_str(include_str!("../assets/docs/template.md")),
            Doc::Other => md.push_str(include_str!("../assets/docs/other.md")),
        }
    }

    if !md.is_empty() {
        let mut skin = MadSkin::default();
        skin.bold.set_fg(Color::Yellow);
        skin.print_text(&md);
        exit(0)
    }
}

fn handle_frecency_cli(args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    match args[0].as_str() {
        "add" => {
            let path = args.get(1).cloned().unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            });
            let store = matchmaker::frecency::FrecencyStore::open();
            match store.add(&path) {
                Ok(score) => {
                    log::info!("Recorded access for '{path}' (frecency score: {score})");
                }
                Err(err) => {
                    log::error!("Failed to record frecency for '{path}': {err}");
                }
            }
            true
        }
        "remove" | "rm" => {
            if let Some(path) = args.get(1) {
                let store = matchmaker::frecency::FrecencyStore::open();
                match store.remove(path) {
                    Ok(true) => {
                        println!("Removed '{path}' from frecency database.");
                    }
                    Ok(false) => {
                        println!("Path '{path}' not found in frecency database.");
                    }
                    Err(err) => {
                        eprintln!("Failed to remove '{path}' from frecency database: {err}");
                    }
                }
            } else {
                eprintln!("Usage: mm remove <path>");
                exit(1);
            }
            true
        }
        "rank" => {
            let path = args.get(1).cloned().unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            });
            let store = matchmaker::frecency::FrecencyStore::open();
            if let Some(record) = store.rank(&path) {
                let score = store.get_bonus(&path);
                println!(
                    "Path: {}\nScore: {}\nCount: {}\nLast Accessed: {}",
                    record.path, score, record.count, record.last_accessed
                );
            } else {
                println!("Path '{}' not found in frecency database", path);
            }
            true
        }
        "list" | "query" => {
            let keywords: Vec<String> = args
                .iter()
                .skip(1)
                .filter(|a| *a != "-l" && *a != "--list" && !a.starts_with('-'))
                .map(|a| a.to_lowercase())
                .collect();

            let store = matchmaker::frecency::FrecencyStore::open();
            let snapshot = store.get_snapshot();
            let mut entries: Vec<_> = snapshot.scores.into_iter().collect();
            // Sort by score descending
            entries.sort_by(|a, b| b.1.cmp(&a.1));

            for (path, _score) in entries {
                let lower_path = path.to_lowercase();
                let matches_all = keywords.iter().all(|kw| lower_path.contains(kw));
                if matches_all {
                    println!("{path}");
                }
            }
            true
        }
        "init" => {
            let shell = args.get(1).map(|s| s.as_str()).unwrap_or("zsh");
            let mut cmd_name = "z";
            for (idx, arg) in args.iter().enumerate() {
                if arg == "--cmd" {
                    if let Some(val) = args.get(idx + 1) {
                        cmd_name = val.as_str();
                    }
                } else if let Some(val) = arg.strip_prefix("--cmd=") {
                    cmd_name = val;
                }
            }

            let raw_script = match shell {
                "zsh" => include_str!("shell/zsh.sh"),
                "bash" => include_str!("shell/bash.sh"),
                "fish" => include_str!("shell/fish.fish"),
                "nushell" | "nu" => include_str!("shell/nu.nu"),
                "powershell" | "pwsh" => include_str!("shell/pwsh.ps1"),
                _ => {
                    eprintln!("Unsupported shell '{shell}'. Supported shells: zsh, bash, fish, nushell, powershell");
                    exit(1);
                }
            };

            let mut script = if cmd_name != "z" {
                raw_script
                    .replace("z()", &format!("{cmd_name}()"))
                    .replace("zi()", &format!("{cmd_name}i()"))
                    .replace("function z\n", &format!("function {cmd_name}\n"))
                    .replace("function zi\n", &format!("function {cmd_name}i\n"))
                    .replace("def --env z ", &format!("def --env {cmd_name} "))
                    .replace("function z ", &format!("function {cmd_name} "))
            } else {
                raw_script.to_string()
            };

            if cmd_name != "z" {
                script.push_str(&format!("\nalias z={cmd_name} 2>/dev/null\nalias zi={cmd_name}i 2>/dev/null\n"));
            }

            println!("{script}");
            true
        }
        "import" => {
            let target = args.get(1).map(|s| s.as_str()).unwrap_or("zoxide");
            if target == "zoxide" {
                import_zoxide();
            } else {
                eprintln!("Unknown import target '{target}'. Supported targets: zoxide");
                exit(1);
            }
            true
        }
        "clean" | "prune" => {
            let store = matchmaker::frecency::FrecencyStore::open();
            match store.clean_stale() {
                Ok(count) => {
                    println!("Cleaned {count} stale entries from frecency database.");
                }
                Err(err) => {
                    eprintln!("Failed to clean frecency database: {err}");
                    exit(1);
                }
            }
            true
        }
        "cache" => {
            let start = std::time::Instant::now();
            let root = args.get(1).map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("."));
            let (count, cache_file) = cache_index(&root);
            let elapsed = start.elapsed();
            println!("Cached {count} entries into {} in {:.2?}", cache_file.display(), elapsed);
            true
        }
        _ => false,
    }
}

fn cache_index(root: &std::path::Path) -> (usize, std::path::PathBuf) {
    let cache_dir = state_dir();
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_file = cache_dir.join("index_cache.txt");

    let mut count = 0;
    if let Ok(mut file) = std::fs::File::create(&cache_file) {
        use std::io::Write;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let path_str = path.display().to_string();
                    let _ = writeln!(file, "{path_str}");
                    count += 1;
                    if path.is_dir() && !path_str.contains("/.") && !path_str.contains("node_modules") && !path_str.contains("/target/") {
                        stack.push(path);
                    }
                }
            }
        }
    }
    (count, cache_file)
}

fn import_zoxide() {
    let store = matchmaker::frecency::FrecencyStore::open();
    let mut imported_count = 0;

    if let Ok(output) = std::process::Command::new("zoxide")
        .args(["query", "-l", "-s"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(score) = parts[0].parse::<f64>() {
                        let path = parts[1..].join(" ");
                        let weight = score.round() as u64;
                        if store.import_entry(&path, weight).is_ok() {
                            imported_count += 1;
                        }
                    }
                }
            }
        }
    }

    if imported_count == 0 {
        let zoxide_db_path = dirs::data_local_dir()
            .map(|d| d.join("zoxide").join("db.zo"))
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share/zoxide/db.zo")));

        if let Some(db_path) = zoxide_db_path {
            if db_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&db_path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && std::path::Path::new(trimmed).exists() {
                            if store.import_entry(trimmed, 1).is_ok() {
                                imported_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if imported_count > 0 {
        println!("Successfully imported {imported_count} entries from zoxide into matchmaker frecency database.");
    } else {
        println!("No zoxide entries found to import.");
    }
}
