use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use chrono::{DateTime, Local};
use humansize::{format_size, DECIMAL};
use sysinfo::Disks;
use std::os::windows::fs::MetadataExt;

#[derive(Clone, Debug, PartialEq)]
pub enum FileType {
    Directory,
    File,
    Symlink,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub file_type: FileType,
    pub size: String,
    pub raw_size: u64,
    pub modified: String,
    pub is_hidden: bool,
}

pub fn get_drives() -> Vec<PathBuf> {
    let disks = Disks::new_with_refreshed_list();
    disks.list().iter().map(|disk| disk.mount_point().to_path_buf()).collect()
}

pub fn read_directory(path: &Path) -> Result<Vec<FileEntry>, String> {
    let mut entries = Vec::new();

    match fs::read_dir(path) {
        Ok(read_dir) => {
            for entry_result in read_dir {
                if let Ok(entry) = entry_result {
                    let path = entry.path();
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue, // Skip files we can't stat
                    };

                    let name = entry.file_name().to_string_lossy().to_string();
                    
                    // Windows specific hidden check
                    let is_hidden = (metadata.file_attributes() & 0x2) != 0;

                    let file_type = if metadata.is_dir() {
                        FileType::Directory
                    } else if metadata.is_symlink() {
                        FileType::Symlink
                    } else {
                        FileType::File
                    };

                    let size = if metadata.is_dir() {
                        "-".to_string()
                    } else {
                        format_size(metadata.len(), DECIMAL)
                    };
                    
                    let raw_size = if metadata.is_dir() { 0 } else { metadata.len() };

                    let modified: DateTime<Local> = metadata.modified().unwrap_or(SystemTime::now()).into();
                    let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();

                    entries.push(FileEntry {
                        name,
                        path,
                        file_type,
                        size,
                        raw_size,
                        modified: modified_str,
                        is_hidden,
                    });
                }
            }
        }
        Err(e) => return Err(e.to_string()),
    }

    // Sort: Directories first, then files. Alphabetical within groups.
    entries.sort_by(|a, b| {
        match (a.file_type == FileType::Directory, b.file_type == FileType::Directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(entries)
}

pub fn delete_entry(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

pub fn rename_entry(old_path: &Path, new_name: &str) -> Result<(), String> {
    let parent = old_path.parent().ok_or("No parent directory")?;
    let new_path = parent.join(new_name);
    fs::rename(old_path, new_path).map_err(|e| e.to_string())
}

pub fn copy_entry(src: &Path, dest_dir: &Path) -> Result<(), String> {
    let file_name = src.file_name().ok_or("Invalid source name")?;
    let dest_path = dest_dir.join(file_name);

    if src.is_dir() {
        copy_dir_recursive(src, &dest_path)
    } else {
        fs::copy(src, dest_path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !dst.exists() {
        fs::create_dir(dst).map_err(|e| e.to_string())?;
    }

    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path();
        let dest_path = dst.join(entry.file_name());

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dest_path)?;
        } else {
            fs::copy(&entry_path, &dest_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn create_directory(parent: &Path, name: &str) -> Result<(), String> {
    let path = parent.join(name);
    if path.exists() {
        return Err("Directory already exists".to_string());
    }
    fs::create_dir(path).map_err(|e| e.to_string())
}

pub fn create_file(parent: &Path, name: &str) -> Result<(), String> {
    let path = parent.join(name);
    if path.exists() {
        return Err("File already exists".to_string());
    }
    fs::File::create(path).map_err(|e| e.to_string())?;
    Ok(())
}