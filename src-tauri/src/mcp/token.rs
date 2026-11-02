//! The per-user token guarding the localhost HTTP transport.

/// Generate a fresh random token: 16 random bytes as 32 hex characters.
/// Entropy comes from the OS (`getrandom`), never from the clock alone —
/// another process on the same machine must not be able to guess it.
pub fn generate() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("Failed to read OS entropy: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Compare two tokens without leaking where they first differ. Both tokens
/// are local secrets, but a constant-time compare costs nothing.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_hex_and_unique() {
        let a = generate().unwrap();
        let b = generate().unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn constant_time_eq_matches_only_identical_tokens() {
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secret", "secreT"));
        assert!(!constant_time_eq("secret", "secrets"));
        assert!(!constant_time_eq("", "secret"));
    }
}
