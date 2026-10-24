use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub(crate) const MAX_ACCOUNT_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) fn read_limited(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_ACCOUNT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_ACCOUNT_BYTES,
        "Account file is too large"
    );
    Ok(bytes)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("File has no parent directory")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("Failed to save {}", path.display()))
}

#[cfg(windows)]
pub(crate) fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into()?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI owns the output allocation; copy it before releasing it with LocalFree.
    unsafe {
        anyhow::ensure!(
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output
            ) != 0,
            "Windows could not encrypt the account store: {}",
            std::io::Error::last_os_error()
        );
        let mut encrypted = b"CSL1".to_vec();
        encrypted.extend_from_slice(std::slice::from_raw_parts(
            output.pbData,
            output.cbData as usize,
        ));
        LocalFree(output.pbData.cast());
        Ok(encrypted)
    }
}

#[cfg(windows)]
pub(crate) fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let encrypted = bytes
        .strip_prefix(b"CSL1")
        .context("Invalid account store; use Import existing accounts for an upstream JSON file")?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len().try_into()?,
        pbData: encrypted.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        anyhow::ensure!(
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output
            ) != 0,
            "Windows could not decrypt the account store for this user: {}",
            std::io::Error::last_os_error()
        );
        let plaintext = if output.cbData == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
        };
        LocalFree(output.pbData.cast());
        Ok(plaintext)
    }
}

#[cfg(not(windows))]
pub(crate) fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(bytes.to_vec())
}

#[cfg(not(windows))]
pub(crate) fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_replacement_preserves_destination_and_removes_temporary_file() {
        let root = std::env::temp_dir().join(format!("switcher-atomic-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let target = root.join("accounts");
        atomic_write(&target, b"old").unwrap();
        atomic_write(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        let directory = root.join("directory");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("keep"), b"untouched").unwrap();
        assert!(atomic_write(&directory, b"invalid replacement").is_err());
        assert_eq!(fs::read(directory.join("keep")).unwrap(), b"untouched");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_round_trip_hides_secrets_and_rejects_tampering() {
        let secret = b"synthetic-refresh-token";
        let mut encrypted = protect(secret).unwrap();
        assert!(!encrypted
            .windows(secret.len())
            .any(|window| window == secret));
        assert_eq!(unprotect(&encrypted).unwrap(), secret);
        *encrypted.last_mut().unwrap() ^= 1;
        assert!(unprotect(&encrypted).is_err());
        assert!(unprotect(br#"{"accounts":[]}"#).is_err());
    }
}
