//! OTEL-compatible hex ID generation for trace and span correlation.

/// Generate a trace ID: 16 random bytes → 32 lowercase hex chars.
pub fn gen_trace_id() -> String {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("failed to generate random bytes");
    hex_encode(&buf)
}

/// Generate a span ID: 8 random bytes → 16 lowercase hex chars.
pub fn gen_span_id() -> String {
    let mut buf = [0u8; 8];
    getrandom::getrandom(&mut buf).expect("failed to generate random bytes");
    hex_encode(&buf)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_format() {
        let id = gen_trace_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn span_id_format() {
        let id = gen_span_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ids_are_unique() {
        let a = gen_trace_id();
        let b = gen_trace_id();
        assert_ne!(a, b);
    }
}
