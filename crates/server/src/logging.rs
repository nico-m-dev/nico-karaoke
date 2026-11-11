//! Log-file retention. Rotation happens when the server starts so a long-running session
//! never leaves an unbounded file behind between restarts.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Rotate once the current log passes 5 MiB.
pub const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
/// Keep the current file plus three older files.
pub const LOG_BACKUPS: usize = 3;

/// Rename an oversized log to `.1`, shifting older backups down. Missing files are fine.
/// Returns whether the log was rotated.
pub fn rotate_if_needed(path: &Path, max_bytes: u64, backups: usize) -> io::Result<bool> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            prune_stale_backups(path, backups)?;
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() < max_bytes {
        prune_stale_backups(path, backups)?;
        return Ok(false);
    }

    for index in (1..=backups).rev() {
        let source = if index == 1 {
            path.to_path_buf()
        } else {
            backup_path(path, index - 1)
        };
        if !source.is_file() {
            continue;
        }
        let target = backup_path(path, index);
        if target.exists() {
            fs::remove_file(&target)?;
        }
        fs::rename(source, target)?;
    }
    prune_stale_backups(path, backups)?;

    Ok(true)
}

/// Remove numbered backups beyond the retention limit, including files left by an older
/// policy. Only `<name>.<digits>` files are touched.
fn prune_stale_backups(path: &Path, backups: usize) -> io::Result<()> {
    let (Some(parent), Some(name)) = (
        path.parent(),
        path.file_name().and_then(|value| value.to_str()),
    ) else {
        return Ok(());
    };
    let prefix = format!("{name}.");
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(suffix) = file_name.strip_prefix(&prefix) else {
            continue;
        };
        if suffix.chars().all(|character| character.is_ascii_digit())
            && suffix.parse::<usize>().is_ok_and(|index| index > backups)
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn backup_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_a_small_or_missing_log_alone() {
        let temp = tempfile::tempdir().expect("temp directory");
        let log = temp.path().join("app.log");
        assert!(!rotate_if_needed(&log, 10, 2).expect("missing file is fine"));

        fs::write(&log, b"small").expect("write log");
        assert!(!rotate_if_needed(&log, 10, 2).expect("small file is fine"));
        assert_eq!(fs::read(&log).expect("log exists"), b"small");
        assert!(!backup_path(&log, 1).exists());
    }

    #[test]
    fn shifts_backups_and_keeps_the_newest_log() {
        let temp = tempfile::tempdir().expect("temp directory");
        let log = temp.path().join("app.log");
        fs::write(&log, b"current").expect("write current");
        fs::write(backup_path(&log, 1), b"one").expect("write one");
        fs::write(backup_path(&log, 2), b"two").expect("write two");

        assert!(rotate_if_needed(&log, 4, 3).expect("rotate"));
        assert!(!log.exists());
        assert_eq!(
            fs::read(backup_path(&log, 1)).expect("backup one"),
            b"current"
        );
        assert_eq!(fs::read(backup_path(&log, 2)).expect("backup two"), b"one");
        assert_eq!(
            fs::read(backup_path(&log, 3)).expect("backup three"),
            b"two"
        );
    }

    #[test]
    fn a_small_log_still_prunes_stale_backups() {
        let temp = tempfile::tempdir().expect("temp directory");
        let log = temp.path().join("app.log");
        fs::write(&log, b"small").expect("write log");
        fs::write(backup_path(&log, 4), b"stale").expect("write stale");

        assert!(!rotate_if_needed(&log, 10, 3).expect("small file is fine"));
        assert_eq!(fs::read(&log).expect("log exists"), b"small");
        assert!(!backup_path(&log, 4).exists());
    }

    #[test]
    fn drops_the_oldest_backup_beyond_the_limit() {
        let temp = tempfile::tempdir().expect("temp directory");
        let log = temp.path().join("app.log");
        fs::write(&log, b"current").expect("write current");
        fs::write(backup_path(&log, 1), b"one").expect("write one");
        fs::write(backup_path(&log, 2), b"two").expect("write two");
        fs::write(backup_path(&log, 3), b"three").expect("write three");

        assert!(rotate_if_needed(&log, 4, 3).expect("rotate"));
        assert_eq!(
            fs::read(backup_path(&log, 1)).expect("backup one"),
            b"current"
        );
        assert_eq!(fs::read(backup_path(&log, 2)).expect("backup two"), b"one");
        assert_eq!(
            fs::read(backup_path(&log, 3)).expect("backup three"),
            b"two"
        );
        assert_eq!(
            fs::read_dir(temp.path()).expect("list directory").count(),
            3
        );
    }
}
