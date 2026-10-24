//! The bundled driver installers, described once in `drivers/manifest.json`.
//! build.rs verifies the files against this at build time and generates the
//! NSIS hook from it; the app verifies the same hashes at run time before it
//! offers to run an installer. One file, no drift.

use serde::{Deserialize, Serialize};

use crate::sha256::sha256_hex;

pub const MANIFEST_JSON: &str = include_str!("../../drivers/manifest.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub installers: Vec<InstallerSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstallerSpec {
    /// Short id used by the UI and the engine ("vigem", "hidhide").
    pub key: String,
    pub name: String,
    pub version: String,
    /// File name inside the `drivers` folder.
    pub file: String,
    pub sha256: String,
    pub size: u64,
    pub silent_args: String,
    pub required: bool,
    pub license: String,
    pub source: String,
    pub purpose: String,
}

pub fn manifest() -> Manifest {
    serde_json::from_str(MANIFEST_JSON).expect("drivers/manifest.json must be valid")
}

/// Byte-exact check of a downloaded installer against its manifest entry.
pub fn verify(spec: &InstallerSpec, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != spec.size {
        return Err(format!("size is {} bytes, expected {}", bytes.len(), spec.size));
    }
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(&spec.sha256) {
        return Err(format!("SHA-256 is {actual}, expected {}", spec.sha256));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_and_is_well_formed() {
        let m = manifest();
        assert!(m.installers.iter().any(|s| s.key == "vigem" && s.required));
        assert!(m.installers.iter().any(|s| s.key == "hidhide" && !s.required));
        for s in &m.installers {
            assert_eq!(s.sha256.len(), 64, "{}: sha256 must be 64 hex chars", s.key);
            assert!(s.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(s.file.ends_with(".exe"));
            assert!(!s.silent_args.is_empty());
            assert!(s.license.ends_with(".txt"));
            assert!(s.source.starts_with("https://github.com/nefarius/"));
            // A file name that could break out of the drivers folder or the NSIS quoting.
            assert!(!s.file.contains(['\\', '/', '"', '$']));
        }
        let keys: Vec<&str> = m.installers.iter().map(|s| s.key.as_str()).collect();
        let mut dedup = keys.clone();
        dedup.dedup();
        assert_eq!(keys, dedup, "keys must be unique");
    }

    #[test]
    fn verify_checks_size_then_hash() {
        let spec = InstallerSpec {
            key: "x".into(),
            name: "X".into(),
            version: "1".into(),
            file: "x.exe".into(),
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            size: 3,
            silent_args: "/q".into(),
            required: true,
            license: "L.txt".into(),
            source: "https://github.com/nefarius/x".into(),
            purpose: String::new(),
        };
        assert_eq!(verify(&spec, b"abc"), Ok(()));
        assert!(verify(&spec, b"abcd").unwrap_err().contains("size"));
        assert!(verify(&spec, b"abd").unwrap_err().contains("SHA-256"));
    }
}
