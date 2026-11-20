use anyhow::{Context, Result};
use std::{
    fs::TryLockError,
    fs::{self, File, OpenOptions},
    path::Path,
};

pub(crate) fn acquire() -> Result<Option<File>> {
    // Do not use SCRY_DATA_ROOT: installed and development builds must share the lock.
    let dirs = directories::ProjectDirs::from("app", "Scry", "Scry")
        .context("Cannot locate application instance folder")?;
    acquire_in(dirs.data_local_dir())
}

fn acquire_in(directory: &Path) -> Result<Option<File>> {
    fs::create_dir_all(directory).context("Cannot create application instance folder")?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("instance.lock"))
        .context("Cannot open application instance lock")?;
    // Keep the file in place: unlinking it can let another process lock a new file.
    // Closing the handle releases the OS lock, including after a crash.
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => {
            Err(error).context("Cannot acquire application instance lock")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn excludes_other_processes_and_allows_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let instance =
            acquire_in(directory.path())?.context("First launch must acquire the lock")?;
        attempt_in_child(directory.path(), false)?;
        drop(instance);
        attempt_in_child(directory.path(), true)?;
        Ok(())
    }

    fn attempt_in_child(directory: &Path, expected: bool) -> Result<()> {
        let status = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "single_instance::tests::child_attempt",
                "--nocapture",
            ])
            .env("SCRY_INSTANCE_TEST_DIRECTORY", directory)
            .env("SCRY_INSTANCE_TEST_EXPECTED", expected.to_string())
            .status()?;
        assert!(status.success(), "Child launch failed: {status}");
        Ok(())
    }

    #[test]
    fn child_attempt() -> Result<()> {
        let Some(directory) = std::env::var_os("SCRY_INSTANCE_TEST_DIRECTORY") else {
            return Ok(());
        };
        let expected = std::env::var("SCRY_INSTANCE_TEST_EXPECTED")? == "true";
        assert_eq!(
            acquire_in(Path::new(&directory))?.is_some(),
            expected,
            "Only the first process may acquire the application instance lock"
        );
        Ok(())
    }

    #[test]
    fn lock_errors_do_not_allow_startup() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("instance.lock"))?;
        assert!(acquire_in(directory.path()).is_err());
        Ok(())
    }
}
