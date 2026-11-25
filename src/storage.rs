use std::io::Write;
use std::path::Path;

/// Publish a complete file in a single rename on the destination filesystem.
pub fn atomic_write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let write = || -> std::io::Result<()> {
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(path)?;
        Ok(())
    };
    write().map_err(|err| format!("No se pudo guardar {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_replacement_leaves_destination_and_directory_intact() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("carpeta");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("original"), b"original").unwrap();
        assert!(atomic_write_file(&target, b"nuevo").is_err());
        assert_eq!(std::fs::read(target.join("original")).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn locked_pdf_keeps_its_original_bytes_after_failed_save() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("plano.pdf");
        std::fs::write(&path, b"original").unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(atomic_write_file(&path, b"nuevo").is_err());
        drop(locked);
        assert_eq!(std::fs::read(path).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
