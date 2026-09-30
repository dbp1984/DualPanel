use std::{fs, io::Write, path::{Path, PathBuf}};
use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Favorite { pub name: String, pub path: PathBuf }

fn store_path() -> PathBuf {
    if let Ok(p) = std::env::var("DBP_FAVORITES_FILE") { return PathBuf::from(p); }
    #[cfg(target_os="windows")]
    if let Ok(p) = std::env::var("APPDATA") { return PathBuf::from(p).join("DBP").join("favorites-v3.tsv"); }
    #[cfg(target_os="macos")]
    if let Ok(h) = std::env::var("HOME") { return PathBuf::from(h).join("Library/Application Support/DBP/favorites-v3.tsv"); }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dbp").join("favorites-v3.tsv")
}

pub fn load() -> Vec<Favorite> {
    let p = store_path();
    let Ok(s) = fs::read_to_string(p) else { return vec![] };
    s.lines().filter_map(|line| {
        let (name, path) = line.split_once('\t')?;
        Some(Favorite { name: name.replace("\\t", "\t"), path: PathBuf::from(path) })
    }).collect()
}

pub fn save(items: &[Favorite]) -> Result<()> {
    let p = store_path();
    if let Some(parent) = p.parent() { fs::create_dir_all(parent)?; }
    let mut f = fs::File::create(&p).with_context(|| format!("Cannot write {}", p.display()))?;
    for item in items {
        writeln!(f, "{}\t{}", item.name.replace('\t', "\\t"), item.path.display())?;
    }
    Ok(())
}

pub fn add(items: &mut Vec<Favorite>, path: &Path) -> Result<()> {
    let name = path.file_name().and_then(|x| x.to_str()).filter(|s| !s.is_empty()).unwrap_or("Root").to_string();
    if let Some(existing) = items.iter_mut().find(|f| f.path == path) { existing.name = name; }
    else { items.push(Favorite { name, path: path.to_path_buf() }); }
    save(items)
}
