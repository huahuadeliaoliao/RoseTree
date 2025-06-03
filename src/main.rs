use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Local;
use dashmap::DashMap;
use ignore::WalkBuilder;
use rayon::prelude::*;
use tokio::fs as async_fs;
use tokio::io::AsyncReadExt;

#[derive(Clone)]
struct FileInfo {
    path: PathBuf,
    relative_path: String,
    extension: String,
}

#[derive(Clone)]
struct GitIgnoreInfo {
    relative_path: String,
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Scanning current directory and subdirectories...");

    let current_dir =
        std::env::current_dir().map_err(|e| format!("Unable to get current directory: {e}"))?;

    let gitignore_files = find_gitignore_files(&current_dir)?;

    let use_gitignore = if gitignore_files.is_empty() {
        false
    } else {
        println!("\nFound the following .gitignore files:");
        for info in &gitignore_files {
            println!("  - {}", info.relative_path);
        }

        println!("\nApply .gitignore rules? (y/n):");
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        input.trim().to_lowercase() == "y"
    };

    let files = if use_gitignore {
        collect_files_with_gitignore(&current_dir)
    } else {
        collect_files_without_gitignore(&current_dir)?
    };

    if files.is_empty() {
        println!("No UTF-8 readable files found.");
        return Ok(());
    }

    let extensions: Vec<String> = files
        .iter()
        .map(|f| f.extension.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    println!("\nFound the following UTF-8 file types:");
    for (i, ext) in extensions.iter().enumerate() {
        println!(
            "{}. {}",
            i + 1,
            if ext.is_empty() { "no extension" } else { ext }
        );
    }

    println!("\nEnter file type numbers to extract (space-separated, 'all' for all types):");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read input: {e}"))?;

    let selected_extensions: HashSet<String> = if input.trim().to_lowercase() == "all" {
        extensions.into_iter().collect()
    } else {
        input
            .split_whitespace()
            .filter_map(|s| s.parse::<usize>().ok())
            .filter_map(|i| extensions.get(i.saturating_sub(1)).cloned())
            .collect()
    };

    if selected_extensions.is_empty() {
        println!("No file types selected.");
        return Ok(());
    }

    let selected_files: Vec<FileInfo> = files
        .into_par_iter()
        .filter(|f| selected_extensions.contains(&f.extension))
        .collect();

    if selected_files.is_empty() {
        println!("No matching files found.");
        return Ok(());
    }

    println!("\nReading file contents...");

    let mut tasks = Vec::new();
    for file in &selected_files {
        let file = file.clone();
        let task = tokio::spawn(async move {
            match read_file_content(&file.path).await {
                Ok(content) => Some((file, content)),
                Err(_) => None,
            }
        });
        tasks.push(task);
    }

    let mut file_contents = Vec::new();
    for task in tasks {
        if let Ok(Some(result)) = task.await {
            file_contents.push(result);
        }
    }

    file_contents.sort_by(|a, b| a.0.relative_path.cmp(&b.0.relative_path));

    let tree_structure = generate_tree_structure(&file_contents);

    let mut final_content = String::new();

    final_content.push_str("File Structure:\n");
    final_content.push_str(&tree_structure);
    final_content.push_str("\n\n");
    final_content.push_str(&"=".repeat(80));
    final_content.push_str("\nFile Contents:\n");
    final_content.push_str(&"=".repeat(80));
    final_content.push_str("\n\n");

    for (i, (file, content)) in file_contents.iter().enumerate() {
        final_content.push_str(&format!("{}:\n", file.relative_path));
        final_content.push_str(&"-".repeat(80));
        final_content.push('\n');
        final_content.push_str(content);
        final_content.push('\n');

        if i < file_contents.len() - 1 {
            final_content.push_str(&"=".repeat(80));
            final_content.push_str("\n\n");
        }
    }

    let current_time = Local::now();
    let timestamp_str = current_time.format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("rosetree_{timestamp_str}.txt");

    async_fs::write(&filename, final_content)
        .await
        .map_err(|e| format!("Failed to write file: {e}"))?;

    println!("\nFile contents successfully extracted to: {filename}");

    Ok(())
}

fn walk_for_gitignore_recursive(
    dir: &Path,
    base_dir: &Path,
    gitignore_files: &mut Vec<GitIgnoreInfo>,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
                continue;
            }
            walk_for_gitignore_recursive(&path, base_dir, gitignore_files)?;
        } else if path.file_name().and_then(|n| n.to_str()) == Some(".gitignore") {
            let relative_path = path
                .strip_prefix(base_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            gitignore_files.push(GitIgnoreInfo { relative_path });
        }
    }
    Ok(())
}

fn find_gitignore_files(base_dir: &Path) -> Result<Vec<GitIgnoreInfo>, Box<dyn std::error::Error>> {
    let mut gitignore_files = Vec::new();
    walk_for_gitignore_recursive(base_dir, base_dir, &mut gitignore_files)?;
    Ok(gitignore_files)
}

fn collect_files_with_gitignore(base_dir: &Path) -> Vec<FileInfo> {
    let mut files = Vec::new();

    let walker = WalkBuilder::new(base_dir)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .ignore(true)
        .hidden(false)
        .follow_links(false)
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();

                if path.is_dir() {
                    continue;
                }

                if path.components().any(|c| c.as_os_str() == ".git") {
                    continue;
                }

                if !is_utf8_file(path) {
                    continue;
                }

                let relative_path = path
                    .strip_prefix(base_dir)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");

                let extension = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_string();

                files.push(FileInfo {
                    path: path.to_path_buf(),
                    relative_path,
                    extension,
                });
            }
            Err(err) => {
                eprintln!("Warning: Error walking directory: {err}");
            }
        }
    }
    files
}

fn collect_files_without_gitignore(
    base_dir: &Path,
) -> Result<Vec<FileInfo>, Box<dyn std::error::Error>> {
    let files = Arc::new(DashMap::new());

    collect_files_recursive(base_dir, base_dir, &files)?;

    Ok(files.iter().map(|entry| entry.value().clone()).collect())
}

fn collect_files_recursive(
    dir: &Path,
    base_dir: &Path,
    files: &Arc<DashMap<PathBuf, FileInfo>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = fs::read_dir(dir)?;

    let paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();

    paths.par_iter().for_each(|path| {
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
                return;
            }
            let _ = collect_files_recursive(path, base_dir, files);
        } else if path.is_file() && is_utf8_file(path) {
            let relative_path = path
                .strip_prefix(base_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();

            let file_info = FileInfo {
                path: path.clone(),
                relative_path,
                extension,
            };

            files.insert(path.clone(), file_info);
        }
    });

    Ok(())
}

fn is_utf8_file(path: &Path) -> bool {
    match fs::read(path) {
        Ok(bytes) => {
            let sample_size = std::cmp::min(8192, bytes.len());
            std::str::from_utf8(&bytes[..sample_size]).is_ok()
        }
        Err(_) => false,
    }
}

async fn read_file_content(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = async_fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open file {path:?}: {e}"))?;

    let mut content = String::new();
    file.read_to_string(&mut content)
        .await
        .map_err(|e| format!("Failed to read file content {path:?}: {e}"))?;

    Ok(content)
}

fn generate_tree_structure(files: &[(FileInfo, String)]) -> String {
    let mut tree = String::new();
    let mut path_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_paths: HashSet<String> = HashSet::new();

    for (file, _) in files {
        let parts: Vec<&str> = file.relative_path.split('/').collect();

        for i in 1..parts.len() {
            let dir_path = parts[..i].join("/");
            all_paths.insert(dir_path);
        }

        all_paths.insert(file.relative_path.clone());
    }

    for path in &all_paths {
        let parts: Vec<&str> = path.split('/').collect();

        if parts.len() == 1 {
            path_map
                .entry(".".to_string())
                .or_default()
                .push(parts[0].to_string());
        } else {
            let parent = parts[..parts.len() - 1].join("/");
            let name = (*parts.last().unwrap()).to_string();
            path_map.entry(parent).or_default().push(name);
        }
    }

    for children in path_map.values_mut() {
        children.sort();
        children.dedup();
    }

    generate_tree_recursive(&path_map, ".", "", &mut tree, true);

    tree
}

fn generate_tree_recursive(
    path_map: &HashMap<String, Vec<String>>,
    current_path: &str,
    prefix: &str,
    output: &mut String,
    is_root: bool,
) {
    if !is_root {
        let name = current_path.split('/').last().unwrap_or(current_path);
        output.push_str(&format!("{prefix}{name}\n"));
    }

    if let Some(children) = path_map.get(current_path) {
        let mut sorted_children = children.clone();
        sorted_children.sort();

        for (i, child) in sorted_children.iter().enumerate() {
            let is_last = i == sorted_children.len() - 1;
            let child_prefix = if is_root {
                if is_last { "└── " } else { "├── " }
            } else {
                &format!("{}{}", prefix, if is_last { "└── " } else { "├── " })
            };

            let child_path = if current_path == "." {
                child.clone()
            } else {
                format!("{current_path}/{child}")
            };

            let is_directory = path_map.contains_key(&child_path);

            if is_directory {
                let new_prefix = if is_root {
                    if is_last { "    " } else { "│   " }
                } else {
                    &format!("{}{}", prefix, if is_last { "    " } else { "│   " })
                };
                generate_tree_recursive(path_map, &child_path, new_prefix, output, false);
            } else {
                output.push_str(&format!("{child_prefix}{child}\n"));
            }
        }
    }
}
