use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Compute SHA256 hash of a file by streaming with 64KB buffers.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Compute a fast spot-check hash by hashing the file size, first `spot_size` bytes,
/// and last `spot_size` bytes. For files smaller than 2 * `spot_size`, the entire
/// file is hashed. Returns a lowercase hex-encoded SHA256 string.
pub fn spot_hash_file(path: &Path, spot_size: u64) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut hasher = Sha256::new();
    hasher.update(file_len.to_le_bytes());

    if file_len == 0 {
        // Nothing more to hash
    } else if file_len <= spot_size * 2 {
        let mut buf = vec![0u8; file_len as usize];
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
    } else {
        let mut head = vec![0u8; spot_size as usize];
        file.read_exact(&mut head)?;
        hasher.update(&head);

        file.seek(SeekFrom::End(-(spot_size as i64)))?;
        let mut tail = vec![0u8; spot_size as usize];
        file.read_exact(&mut tail)?;
        hasher.update(&tail);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Encode bytes as lowercase hex string.
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, Write};
    use tempfile::NamedTempFile;

    #[test]
    fn test_sha256_file_known_hash() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"hello world").unwrap();
        tmp.flush().unwrap();
        let hash = sha256_file(tmp.path()).unwrap();
        // sha256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_sha256_file_empty() {
        let tmp = NamedTempFile::new().unwrap();
        let hash = sha256_file(tmp.path()).unwrap();
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_file_not_found() {
        let result = sha256_file(Path::new("/nonexistent/file"));
        assert!(result.is_err());
    }

    #[test]
    fn test_spot_hash_file_empty() {
        let tmp = NamedTempFile::new().unwrap();
        let hash = spot_hash_file(tmp.path(), 4096).unwrap();
        // Should be deterministic: SHA256 of just the 8-byte little-endian size (0u64)
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 hex length
    }

    #[test]
    fn test_spot_hash_file_small() {
        // File smaller than 2 * spot_size — entire file is hashed
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"hello world").unwrap();
        tmp.flush().unwrap();
        let hash = spot_hash_file(tmp.path(), 4096).unwrap();
        assert_eq!(hash.len(), 64);
        // Should be deterministic
        let hash2 = spot_hash_file(tmp.path(), 4096).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_spot_hash_file_large() {
        // File larger than 2 * spot_size — only head + tail hashed
        let mut tmp = NamedTempFile::new().unwrap();
        let data = vec![0xABu8; 16384]; // 16KB > 2 * 4096
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();
        let hash = spot_hash_file(tmp.path(), 4096).unwrap();
        assert_eq!(hash.len(), 64);
        // Deterministic
        let hash2 = spot_hash_file(tmp.path(), 4096).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_spot_hash_file_detects_middle_change() {
        // Changing the middle of a large file should NOT change the spot hash
        // (this is the expected tradeoff for speed)
        let mut tmp = NamedTempFile::new().unwrap();
        let mut data = vec![0u8; 16384];
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();
        let hash1 = spot_hash_file(tmp.path(), 4096).unwrap();

        // Change a byte in the middle (outside head/tail)
        data[8000] = 0xFF;
        tmp.rewind().unwrap();
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();
        let hash2 = spot_hash_file(tmp.path(), 4096).unwrap();

        // Middle change not detected — same hash (expected behavior)
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_spot_hash_file_detects_head_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        let data = vec![0u8; 16384];
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();
        let hash1 = spot_hash_file(tmp.path(), 4096).unwrap();

        // Change a byte in the head
        let mut data2 = vec![0u8; 16384];
        data2[100] = 0xFF;
        tmp.rewind().unwrap();
        tmp.write_all(&data2).unwrap();
        tmp.flush().unwrap();
        let hash2 = spot_hash_file(tmp.path(), 4096).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_spot_hash_file_detects_tail_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        let data = vec![0u8; 16384];
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();
        let hash1 = spot_hash_file(tmp.path(), 4096).unwrap();

        // Change a byte in the tail
        let mut data2 = vec![0u8; 16384];
        data2[16000] = 0xFF;
        tmp.rewind().unwrap();
        tmp.write_all(&data2).unwrap();
        tmp.flush().unwrap();
        let hash2 = spot_hash_file(tmp.path(), 4096).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_spot_hash_file_detects_size_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&vec![0u8; 16384]).unwrap();
        tmp.flush().unwrap();
        let hash1 = spot_hash_file(tmp.path(), 4096).unwrap();

        // Truncate the file
        tmp.as_file().set_len(16000).unwrap();
        let hash2 = spot_hash_file(tmp.path(), 4096).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_spot_hash_file_different_spot_sizes() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&vec![0u8; 16384]).unwrap();
        tmp.flush().unwrap();

        let hash_small = spot_hash_file(tmp.path(), 1024).unwrap();
        let hash_large = spot_hash_file(tmp.path(), 4096).unwrap();
        // Different spot sizes produce different hashes
        assert_ne!(hash_small, hash_large);
    }

    #[test]
    fn test_spot_hash_file_not_found() {
        let result = spot_hash_file(Path::new("/nonexistent/file"), 4096);
        assert!(result.is_err());
    }

    #[test]
    fn test_spot_hash_file_boundary_exact_double() {
        // File exactly 2 * spot_size — entire file is hashed (no seek needed)
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&vec![0xCDu8; 8192]).unwrap();
        tmp.flush().unwrap();
        let hash = spot_hash_file(tmp.path(), 4096).unwrap();
        assert_eq!(hash.len(), 64);
    }
}
