use chrono::{DateTime, Local};
use humansize::{format_size, DECIMAL};
use std::fs;
use std::io::Write;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use sysinfo::Disks;
use walkdir::WalkDir;
use zip::write::FileOptions;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use pbkdf2::pbkdf2_hmac;
use rand::{RngCore, thread_rng};
use sha2::Sha256;

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
    pub size: u64,
    pub modified: i64,
    pub is_hidden: bool,
}

pub fn encrypt_file(path: &Path, password: &str) -> Result<(), String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    
    let mut salt = [0u8; 16];
    thread_rng().fill_bytes(&mut salt);
    
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, 100_000, &mut key);
    
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    let ciphertext = cipher.encrypt(nonce, data.as_ref()).map_err(|e| e.to_string())?;
    
    let mut output = Vec::with_capacity(salt.len() + nonce_bytes.len() + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    
        let encrypted_path = path.with_extension(format!(
    
            "{}.enc",
    
            path.extension().unwrap_or_default().to_string_lossy()
    
        ));
    
        fs::write(encrypted_path, output).map_err(|e| e.to_string())?;
    
    
    
        // Delete the original file after successful encryption
    
        fs::remove_file(path).map_err(|e| e.to_string())?;
    
    
    
        Ok(())
    
    }
    
    
    
    pub fn decrypt_file(path: &Path, password: &str) -> Result<(), String> {
    
        let data = fs::read(path).map_err(|e| e.to_string())?;
    
        if data.len() < 28 {
    
            return Err("Invalid encrypted file".to_string());
    
        }
    
    
    
        let salt = &data[..16];
    
        let nonce_bytes = &data[16..28];
    
        let ciphertext = &data[28..];
    
    
    
        let mut key = [0u8; 32];
    
        pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, 100_000, &mut key);
    
    
    
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    
        let nonce = Nonce::from_slice(nonce_bytes);
    
    
    
        let plaintext = cipher
    
            .decrypt(nonce, ciphertext)
    
            .map_err(|_| "Decryption failed (wrong password?)".to_string())?;
    
    
    
        let mut new_path = path.to_path_buf();
    
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
    
        if filename.ends_with(".enc") {
    
            let name_without_enc = &filename[..filename.len() - 4];
    
            new_path.set_file_name(name_without_enc);
    
        } else {
    
            new_path.set_extension("decrypted");
    
        }
    
    
    
        fs::write(new_path, plaintext).map_err(|e| e.to_string())?;
    
    
    
        // Delete the encrypted file after successful decryption
    
        fs::remove_file(path).map_err(|e| e.to_string())?;
    
    
    
        Ok(())
    
    }

pub fn get_drives() -> Vec<PathBuf> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .map(|disk| disk.mount_point().to_path_buf())
        .collect()
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

                    let size = if metadata.is_dir() { 0 } else { metadata.len() };

                    let modified = metadata
                        .modified()
                        .unwrap_or(SystemTime::now())
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    entries.push(FileEntry {
                        name,
                        path,
                        file_type,
                        size,
                        modified,
                        is_hidden,
                    });
                }
            }
        }
        Err(e) => return Err(e.to_string()),
    }

    // Sort: Directories first, then files. Alphabetical within groups.
    entries.sort_by(|a, b| {
        match (
            a.file_type == FileType::Directory,
            b.file_type == FileType::Directory,
        ) {
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

pub fn search_directory_recursive(root: &Path, query: &str) -> Vec<FileEntry> {
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.to_lowercase().contains(&query_lower) {
            let path = entry.path().to_path_buf();
            
            if let Ok(metadata) = entry.metadata() {
                 let is_hidden = (metadata.file_attributes() & 0x2) != 0;
                 let file_type = if metadata.is_dir() {
                    FileType::Directory
                } else if metadata.is_symlink() {
                    FileType::Symlink
                } else {
                    FileType::File
                };
                let size = if metadata.is_dir() { 0 } else { metadata.len() };
                
                let modified = metadata
                    .modified()
                    .unwrap_or(SystemTime::now())
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                results.push(FileEntry {
                    name,
                    path,
                    file_type,
                    size,
                    modified,
                    is_hidden,
                });
            }
        }
    }
    results
}

pub fn create_zip(src_path: &Path, dest_path: &Path) -> Result<(), String> {
    let file = fs::File::create(dest_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o755);

    let walk_root = if src_path.is_dir() { src_path } else { src_path.parent().unwrap() };

    if src_path.is_file() {
         let name = src_path.file_name().unwrap().to_string_lossy();
         zip.start_file(name, options).map_err(|e| e.to_string())?;
         let content = fs::read(src_path).map_err(|e| e.to_string())?;
         zip.write_all(&content).map_err(|e| e.to_string())?;
         return zip.finish().map(|_| ()).map_err(|e| e.to_string());
    }

    for entry in WalkDir::new(src_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.strip_prefix(walk_root).unwrap().to_string_lossy().replace("\\", "/");

        if path.is_file() {
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            let content = fs::read(path).map_err(|e| e.to_string())?;
            zip.write_all(&content).map_err(|e| e.to_string())?;
        } else if !name.is_empty() {
             zip.add_directory(name, options).map_err(|e| e.to_string())?;
        }
    }
    zip.finish().map(|_| ()).map_err(|e| e.to_string())
}

pub fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
