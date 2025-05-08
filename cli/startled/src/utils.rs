use anyhow::{anyhow, Result};
use regex::Regex;

/// Validates if a given name is safe to be used as a filesystem directory or file name component.
///
/// A safe name:
/// - Is not empty.
/// - Is not "." or "..".
/// - Contains only alphanumeric characters, underscores (`_`), or hyphens (`-`).
/// - Does not contain filesystem-problematic characters like `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`, or control characters.
pub fn validate_fs_safe_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("Name cannot be empty."));
    }
    if name == "." || name == ".." {
        return Err(anyhow!("Name cannot be '.' or '..'."));
    }

    // Regex to match allowed characters: alphanumeric, underscore, hyphen.
    // Anchored to ensure the entire string matches.
    let safe_chars_regex = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();

    if !safe_chars_regex.is_match(name) {
        return Err(anyhow!(
            "Name '{}' contains invalid characters. Only alphanumeric characters, underscores, and hyphens are allowed.",
            name
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_fs_safe_name_valid() {
        assert!(validate_fs_safe_name("valid-name_123").is_ok());
        assert!(validate_fs_safe_name("another_valid").is_ok());
        assert!(validate_fs_safe_name("simple").is_ok());
    }

    #[test]
    fn test_validate_fs_safe_name_invalid_empty() {
        assert!(validate_fs_safe_name("").is_err());
    }

    #[test]
    fn test_validate_fs_safe_name_invalid_dot() {
        assert!(validate_fs_safe_name(".").is_err());
        assert!(validate_fs_safe_name("..").is_err());
    }

    #[test]
    fn test_validate_fs_safe_name_invalid_chars() {
        assert!(validate_fs_safe_name("invalid/name").is_err());
        assert!(validate_fs_safe_name("invalid*name").is_err());
        assert!(validate_fs_safe_name("invalid:name").is_err());
        assert!(validate_fs_safe_name("invalid name").is_err()); // space
        assert!(validate_fs_safe_name("name!").is_err()); // exclamation
    }
}
