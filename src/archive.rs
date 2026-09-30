use std::{fs::{self, File}, io::{self, Read, Write}, path::{Path, PathBuf}};
use anyhow::{Context, Result};
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

pub fn compress(paths: &[PathBuf], destination: &Path) -> Result<()> {
    let file = File::create(destination).with_context(|| format!("Cannot create {}", destination.display()))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let base = common_parent(paths).unwrap_or_else(|| PathBuf::from("."));
    let mut buf = Vec::new();
    for p in paths {
        if p.is_dir() {
            for e in WalkDir::new(p).follow_links(false).into_iter().filter_map(|e| e.ok()) {
                let path = e.path();
                let name = path.strip_prefix(&base).unwrap_or(path).to_string_lossy().replace('\\', "/");
                if path.is_dir() { if !name.is_empty() { zip.add_directory(format!("{name}/"), opts)?; } }
                else { zip.start_file(name, opts)?; File::open(path)?.read_to_end(&mut buf)?; zip.write_all(&buf)?; buf.clear(); }
            }
        } else {
            let name = p.strip_prefix(&base).unwrap_or(p).to_string_lossy().replace('\\', "/");
            zip.start_file(name, opts)?; File::open(p)?.read_to_end(&mut buf)?; zip.write_all(&buf)?; buf.clear();
        }
    }
    zip.finish()?;
    Ok(())
}

pub fn extract(zip_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut ar = ZipArchive::new(file)?;
    fs::create_dir_all(destination)?;
    for i in 0..ar.len() {
        let mut zf = ar.by_index(i)?;
        let Some(enclosed) = zf.enclosed_name() else { continue };
        let out = destination.join(enclosed);
        if zf.is_dir() { fs::create_dir_all(&out)?; continue; }
        if let Some(parent) = out.parent() { fs::create_dir_all(parent)?; }
        let mut out_f = File::create(&out)?;
        io::copy(&mut zf, &mut out_f)?;
    }
    Ok(())
}

fn common_parent(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.first().and_then(|p| p.parent()).map(Path::to_path_buf)
}
