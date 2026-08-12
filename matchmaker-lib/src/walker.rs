use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use ignore::{WalkBuilder, WalkState};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EntryType {
    Files,
    Directories,
    #[default]
    Any,
}

#[derive(Debug, Clone)]
pub struct WalkerOptions {
    pub root: PathBuf,
    pub hidden: bool,
    pub ignore: bool,
    pub git_exclude: bool,
    pub git_global: bool,
    pub max_depth: Option<usize>,
    pub threads: usize,
    pub entry_type: EntryType,
    pub strip_cwd_prefix: bool,
}

impl Default for WalkerOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            hidden: false,
            ignore: true,
            git_exclude: true,
            git_global: true,
            max_depth: None,
            threads: thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
            entry_type: EntryType::Any,
            strip_cwd_prefix: true,
        }
    }
}

pub struct AsyncWalker {
    options: WalkerOptions,
}

impl AsyncWalker {
    pub fn new(options: WalkerOptions) -> Self {
        Self { options }
    }

    pub fn from_root(root: impl AsRef<Path>) -> Self {
        Self::new(WalkerOptions {
            root: root.as_ref().to_path_buf(),
            ..Default::default()
        })
    }

    pub fn builder(&self) -> WalkBuilder {
        let mut builder = WalkBuilder::new(&self.options.root);
        builder.hidden(!self.options.hidden);
        builder.git_ignore(self.options.ignore);
        builder.git_exclude(self.options.git_exclude);
        builder.git_global(self.options.git_global);
        builder.ignore(self.options.ignore);
        if let Some(depth) = self.options.max_depth {
            builder.max_depth(Some(depth));
        }
        builder.threads(self.options.threads);
        builder
    }

    /// Spawns a parallel walk in a blocking Tokio task, streaming formatted paths to `push_fn`.
    pub fn spawn_walk<F>(&self, mut push_fn: F) -> tokio::task::JoinHandle<()>
    where
        F: FnMut(String) -> Result<(), crate::nucleo::WorkerError> + Send + 'static,
    {
        let options = self.options.clone();
        let builder = self.builder();

        tokio::task::spawn_blocking(move || {
            let (tx, rx) = mpsc::channel::<String>();

            let walker = builder.build_parallel();
            let entry_type = options.entry_type.clone();
            let strip_cwd = options.strip_cwd_prefix;
            let root = options.root.clone();

            thread::spawn(move || {
                walker.run(move || {
                    let tx = tx.clone();
                    let entry_type = entry_type.clone();
                    let root = root.clone();

                    Box::new(move |result| {
                        let Ok(entry) = result else {
                            return WalkState::Continue;
                        };

                        if entry.depth() == 0 {
                            return WalkState::Continue;
                        }

                        let ft = entry.file_type();
                        let is_dir = ft.map_or(false, |t| t.is_dir());
                        let is_file = ft.map_or(false, |t| t.is_file());

                        match entry_type {
                            EntryType::Files if !is_file => return WalkState::Continue,
                            EntryType::Directories if !is_dir => return WalkState::Continue,
                            _ => {}
                        }

                        let path = entry.path();
                        let formatted = format_path(path, &root, strip_cwd, is_dir);
                        let _ = tx.send(formatted);

                        WalkState::Continue
                    })
                });
            });

            while let Ok(line) = rx.recv() {
                if push_fn(line).is_err() {
                    break;
                }
            }
        })
    }

    /// Collects all matching paths synchronously into a `Vec<String>`.
    pub fn collect_sync(&self) -> Vec<String> {
        let mut results = Vec::new();
        let builder = self.builder();
        let walker = builder.build();

        for result in walker {
            let Ok(entry) = result else {
                continue;
            };

            if entry.depth() == 0 {
                continue;
            }

            let ft = entry.file_type();
            let is_dir = ft.map_or(false, |t| t.is_dir());
            let is_file = ft.map_or(false, |t| t.is_file());

            match self.options.entry_type {
                EntryType::Files if !is_file => continue,
                EntryType::Directories if !is_dir => continue,
                _ => {}
            }

            let path = entry.path();
            let formatted = format_path(
                path,
                &self.options.root,
                self.options.strip_cwd_prefix,
                is_dir,
            );
            results.push(formatted);
        }

        results
    }
}

fn format_path(path: &Path, root: &Path, strip_cwd: bool, is_dir: bool) -> String {
    let mut s = if strip_cwd {
        if let Ok(rel) = path.strip_prefix(root) {
            rel.to_string_lossy().to_string()
        } else if let Ok(rel) = path.strip_prefix(".") {
            rel.to_string_lossy().to_string()
        } else {
            path.to_string_lossy().to_string()
        }
    } else {
        path.to_string_lossy().to_string()
    };

    if is_dir && !s.ends_with('/') && !s.ends_with('\\') {
        s.push('/');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_walker_collect_sync() -> anyhow::Result<()> {
        let temp_dir = std::env::temp_dir().join("mm_test_walker");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src"))?;
        fs::write(temp_dir.join("src").join("main.rs"), "fn main() {}")?;
        fs::write(temp_dir.join("README.md"), "# Test")?;

        let options = WalkerOptions {
            root: temp_dir.clone(),
            strip_cwd_prefix: true,
            ..Default::default()
        };

        let walker = AsyncWalker::new(options);
        let items = walker.collect_sync();

        assert!(items.iter().any(|i| i == "src/" || i == "src"));
        assert!(items.iter().any(|i| i == "src/main.rs"));
        assert!(items.iter().any(|i| i == "README.md"));

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_walker_entry_type_filter() -> anyhow::Result<()> {
        let temp_dir = std::env::temp_dir().join("mm_test_walker_filter");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("sub"))?;
        fs::write(temp_dir.join("file.txt"), "hello")?;

        // Files only
        let options_files = WalkerOptions {
            root: temp_dir.clone(),
            entry_type: EntryType::Files,
            ..Default::default()
        };
        let items_files = AsyncWalker::new(options_files).collect_sync();
        assert!(items_files.iter().all(|i| !i.ends_with('/')));
        assert!(items_files.iter().any(|i| i == "file.txt"));

        // Directories only
        let options_dirs = WalkerOptions {
            root: temp_dir.clone(),
            entry_type: EntryType::Directories,
            ..Default::default()
        };
        let items_dirs = AsyncWalker::new(options_dirs).collect_sync();
        assert!(items_dirs.iter().any(|i| i.ends_with('/') || i == "sub"));

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }
}
