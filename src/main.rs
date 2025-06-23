use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::Local;
use content_inspector::inspect;
use dashmap::DashMap;
use ignore::WalkBuilder;
use rayon::prelude::*;

#[derive(Clone)]
struct FileInfo {
    path: PathBuf,
    relative_path: String,
    extension: String,
}

#[derive(Clone, Debug)]
struct GitIgnoreInfo {
    relative_path: String,
}

struct Timings {
    find_gitignore: u128,
    collect_files: u128,
    read_contents: u128,
    generate_tree: u128,
    generate_output_string: u128,
    write_file: u128,
    total: u128,
}

impl Timings {
    fn new() -> Self {
        Timings {
            find_gitignore: 0,
            collect_files: 0,
            read_contents: 0,
            generate_tree: 0,
            generate_output_string: 0,
            write_file: 0,
            total: 0,
        }
    }
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut timings = Timings::new();

    println!("Scanning current directory and subdirectories...");

    let current_dir =
        std::env::current_dir().map_err(|e| format!("Unable to get current directory: {e}"))?;

    let stage_start_time = Instant::now();
    let gitignore_files = find_gitignore_files(&current_dir);
    timings.find_gitignore = stage_start_time.elapsed().as_micros();

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

    let stage_start_time = Instant::now();
    let files = if use_gitignore {
        collect_files_with_gitignore(&current_dir)
    } else {
        collect_files_without_gitignore(&current_dir)
    };
    timings.collect_files = stage_start_time.elapsed().as_micros();

    if files.is_empty() {
        println!("No UTF-8 readable files found.");
        timings.total = timings.find_gitignore + timings.collect_files;
        print_timings(&timings);
        return Ok(());
    }

    let extensions_set: HashSet<String> = files.iter().map(|f| f.extension.clone()).collect();
    let mut extensions_vec: Vec<String> = extensions_set.into_iter().collect();
    extensions_vec.sort();

    println!("\nFound the following UTF-8 file types:");
    for (i, ext) in extensions_vec.iter().enumerate() {
        println!(
            "{}. {}",
            i + 1,
            if ext.is_empty() { "no extension" } else { ext }
        );
    }

    println!("\nEnter file type numbers to extract (space-separated, 'a' for all types):");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read input: {e}"))?;

    let selected_extensions: HashSet<String> = if input.trim().to_lowercase() == "a" {
        extensions_vec.iter().cloned().collect()
    } else {
        input
            .split_whitespace()
            .filter_map(|s| s.parse::<usize>().ok())
            .filter_map(|i| extensions_vec.get(i.saturating_sub(1)).cloned())
            .collect()
    };

    if selected_extensions.is_empty() {
        println!("No file types selected.");
        timings.total = timings.find_gitignore + timings.collect_files;
        print_timings(&timings);
        return Ok(());
    }

    let selected_files: Vec<FileInfo> = files
        .into_par_iter()
        .filter(|f| selected_extensions.contains(&f.extension))
        .collect();

    if selected_files.is_empty() {
        println!("No matching files found.");
        timings.total = timings.find_gitignore + timings.collect_files;
        print_timings(&timings);
        return Ok(());
    }

    println!("\nReading file contents...");
    let stage_start_time = Instant::now();

    let file_contents_results: Vec<Result<(FileInfo, String), String>> = selected_files
        .par_iter()
        .map(
            |file_info_to_read| match fs::read_to_string(&file_info_to_read.path) {
                Ok(content) => Ok((file_info_to_read.clone(), content)),
                Err(e) => {
                    let err_msg = format!(
                        "Warning: Failed to read content of {:?}: {}",
                        file_info_to_read.path, e
                    );
                    eprintln!("{err_msg}");
                    Err(err_msg)
                }
            },
        )
        .collect();

    let mut file_contents: Vec<(FileInfo, String)> = file_contents_results
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    timings.read_contents = stage_start_time.elapsed().as_micros();

    if file_contents.is_empty() && !selected_files.is_empty() {
        println!("All selected files failed to read.");
        timings.total = timings.find_gitignore + timings.collect_files + timings.read_contents;
        print_timings(&timings);
        return Ok(());
    }

    file_contents.sort_by(|a, b| a.0.relative_path.cmp(&b.0.relative_path));

    let stage_start_time = Instant::now();
    let tree_structure = generate_tree_structure(&file_contents);
    timings.generate_tree = stage_start_time.elapsed().as_micros();

    let stage_start_time = Instant::now();
    let estimated_header_footer_size = 1024;
    let estimated_content_size: usize =
        file_contents.iter().map(|(_, content)| content.len()).sum();
    let estimated_path_overhead: usize = file_contents.len() * 200;
    let total_estimated_capacity = tree_structure.len()
        + estimated_content_size
        + estimated_path_overhead
        + estimated_header_footer_size;

    let mut final_content = String::with_capacity(total_estimated_capacity);

    final_content.push_str("File Structure:\n");
    final_content.push_str(&tree_structure);
    final_content.push_str("\n\n");
    final_content.push_str(&"=".repeat(80));
    final_content.push_str("\nFile Contents:\n");
    final_content.push_str(&"=".repeat(80));
    final_content.push_str("\n\n");

    for (i, (file, content)) in file_contents.iter().enumerate() {
        final_content.push_str(&file.relative_path);
        final_content.push_str(":\n");
        final_content.push_str(&"-".repeat(80));
        final_content.push('\n');
        final_content.push_str(content);
        final_content.push('\n');

        if i < file_contents.len() - 1 {
            final_content.push_str(&"=".repeat(80));
            final_content.push_str("\n\n");
        }
    }
    timings.generate_output_string = stage_start_time.elapsed().as_micros();

    let current_time = Local::now();
    let timestamp_str = current_time.format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("rosetree_{timestamp_str}.txt");

    let stage_start_time = Instant::now();
    fs::write(&filename, final_content).map_err(|e| format!("Failed to write file: {e}"))?;
    timings.write_file = stage_start_time.elapsed().as_micros();

    println!("\nFile contents successfully extracted to: {filename}");

    timings.total = timings.find_gitignore
        + timings.collect_files
        + timings.read_contents
        + timings.generate_tree
        + timings.generate_output_string
        + timings.write_file;

    print_timings(&timings);

    Ok(())
}

fn print_timings(timings: &Timings) {
    println!("\nProgram Operation Execution Times (µs):");
    println!("-------------------------------------------");
    println!("Find .gitignore files:     {:>10}", timings.find_gitignore);
    println!("Collect files:             {:>10}", timings.collect_files);
    println!("Read selected contents:    {:>10}", timings.read_contents);
    println!("Generate tree structure:   {:>10}", timings.generate_tree);
    println!(
        "Generate output string:    {:>10}",
        timings.generate_output_string
    );
    println!("Write to file:             {:>10}", timings.write_file);
    println!("-------------------------------------------");
    println!("Total processing time:     {:>10} µs", timings.total);
    println!(
        "                           {:>10} ms (approx total)",
        timings.total / 1000
    );
    println!("-------------------------------------------");
}

fn find_gitignore_files(base_dir: &Path) -> Vec<GitIgnoreInfo> {
    let mut gitignore_files = Vec::new();
    let walker = WalkBuilder::new(base_dir)
        .standard_filters(false)
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_exclude(false)
        .git_ignore(false)
        .filter_entry(|e| e.file_name() != std::ffi::OsStr::new(".git"))
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                if entry.file_type().is_some_and(|ft| ft.is_file())
                    && entry.file_name() == std::ffi::OsStr::new(".gitignore")
                    && !entry.path_is_symlink()
                {
                    let path = entry.path();
                    let relative_path = path
                        .strip_prefix(base_dir)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    gitignore_files.push(GitIgnoreInfo { relative_path });
                }
            }
            Err(err) => {
                eprintln!("Warning: Error finding .gitignore files: {err}");
            }
        }
    }
    gitignore_files
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
                if path.is_dir() || path.components().any(|c| c.as_os_str() == ".git") {
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
                eprintln!("Warning: Error walking directory (with gitignore): {err}");
            }
        }
    }
    files
}

fn collect_files_without_gitignore(base_dir: &Path) -> Vec<FileInfo> {
    let files_map = Arc::new(DashMap::new());
    collect_files_recursive(base_dir, base_dir, &files_map);
    files_map
        .iter()
        .map(|entry| entry.value().clone())
        .collect()
}

fn collect_files_recursive(
    dir: &Path,
    base_dir: &Path,
    files_map: &Arc<DashMap<PathBuf, FileInfo>>,
) {
    let Ok(entries_result) = fs::read_dir(dir) else {
        eprintln!("Warning: Failed to read directory: {dir:?}");
        return;
    };

    let entries: Vec<PathBuf> = entries_result
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();

    entries.into_par_iter().for_each(|path| {
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
                return;
            }
            collect_files_recursive(&path, base_dir, files_map);
        } else if path.is_file() && is_utf8_file(&path) {
            let relative_path = path
                .strip_prefix(base_dir)
                .unwrap_or(&path)
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
            files_map.insert(path.clone(), file_info);
        }
    });
}

fn is_utf8_file(path: &Path) -> bool {
    match fs::File::open(path) {
        Ok(mut file) => {
            // content_inspector只检查前1024字节，所以我们只读取1024字节
            let mut buffer = [0u8; 1024]; 
            match file.read(&mut buffer) {
                Ok(0) => true, // 空文件视为文本文件
                Ok(bytes_read) => inspect(&buffer[..bytes_read]).is_text(),
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

fn generate_tree_structure(files: &[(FileInfo, String)]) -> String {
    let mut path_map: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut all_distinct_paths: HashSet<String> = HashSet::new();

    for (file_info, _) in files {
        let path = Path::new(&file_info.relative_path);
        all_distinct_paths.insert(file_info.relative_path.clone());
        let mut current_accumulated_path = PathBuf::new();
        if let Some(parent_dir) = path.parent() {
            for component in parent_dir.components() {
                if let Some(comp_str) = component.as_os_str().to_str() {
                    if comp_str != "." && comp_str != "/" {
                        current_accumulated_path.push(comp_str);
                        if !current_accumulated_path.as_os_str().is_empty() {
                            all_distinct_paths.insert(
                                current_accumulated_path
                                    .to_string_lossy()
                                    .replace('\\', "/"),
                            );
                        }
                    }
                }
            }
        }
    }

    for path_str in all_distinct_paths {
        let p = Path::new(&path_str);
        let file_name_os = p.file_name().unwrap_or(p.as_os_str());
        let child_name = file_name_os.to_string_lossy().into_owned();

        if let Some(parent_path_os) = p.parent() {
            let parent_key = if parent_path_os.as_os_str().is_empty() {
                ".".to_string()
            } else {
                parent_path_os.to_string_lossy().replace('\\', "/")
            };
            path_map.entry(parent_key).or_default().insert(child_name);
        } else {
            path_map
                .entry(".".to_string())
                .or_default()
                .insert(child_name);
        }
    }
    if files.is_empty()
        && path_map
            .get(".")
            .is_none_or(std::collections::BTreeSet::is_empty)
    {
        return ".\n(No files or directories found to list)\n".to_string();
    } else if files.len() == 1
        && path_map.get(".").is_some_and(|s| s.len() == 1)
        && path_map.get(".").unwrap().iter().next().unwrap() == &files[0].0.relative_path
    {
    } else if !path_map.contains_key(".") && !files.is_empty() {
        for (file_info, _) in files {
            let p = Path::new(&file_info.relative_path);
            if p.parent().is_none_or(|par| par.as_os_str().is_empty()) {
                path_map
                    .entry(".".to_string())
                    .or_default()
                    .insert(p.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }

    let mut output_tree_string = String::new();
    if path_map.contains_key(".") || !files.is_empty() {
        output_tree_string.push_str(".\n");
    }
    generate_tree_recursive(&path_map, ".", "", &mut output_tree_string, true);
    output_tree_string
}

fn generate_tree_recursive(
    path_map: &HashMap<String, BTreeSet<String>>,
    current_path_key: &str,
    prefix_for_children_lines: &str,
    output: &mut String,
    is_current_path_conceptual_root: bool,
) {
    if let Some(children_names) = path_map.get(current_path_key) {
        let num_children = children_names.len();
        for (i, child_name) in children_names.iter().enumerate() {
            let is_last = i == num_children - 1;

            output.push_str(prefix_for_children_lines);
            output.push_str(if is_last { "└── " } else { "├── " });
            output.push_str(child_name);
            output.push('\n');

            let child_full_key = if is_current_path_conceptual_root && current_path_key == "." {
                child_name.clone()
            } else {
                format!("{current_path_key}/{child_name}")
            };

            if path_map.contains_key(&child_full_key) {
                let mut new_prefix_for_grandchildren = prefix_for_children_lines.to_string();
                new_prefix_for_grandchildren.push_str(if is_last { "   " } else { "│  " });
                generate_tree_recursive(
                    path_map,
                    &child_full_key,
                    &new_prefix_for_grandchildren,
                    output,
                    false,
                );
            }
        }
    }
}
