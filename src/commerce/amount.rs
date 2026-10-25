use num_bigint::BigUint;
pub fn atomic(s: &str) -> anyhow::Result<BigUint> {
    anyhow::ensure!(
        !s.is_empty()
            && s.len() <= 78
            && s.bytes().all(|b| b.is_ascii_digit())
            && (s == "0" || !s.starts_with('0')),
        "invalid atomic amount"
    );
    let n =
        BigUint::parse_bytes(s.as_bytes(), 10).ok_or_else(|| anyhow::anyhow!("invalid amount"))?;
    anyhow::ensure!(n.bits() <= 256, "uint256 overflow");
    Ok(n)
}
pub fn add(a: BigUint, b: BigUint) -> anyhow::Result<BigUint> {
    let n = a + b;
    anyhow::ensure!(n.bits() <= 256, "uint256 overflow");
    Ok(n)
}
pub fn multiply(s: &str, q: u32) -> anyhow::Result<BigUint> {
    let n = atomic(s)? * q;
    anyhow::ensure!(n.bits() <= 256, "uint256 overflow");
    Ok(n)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact() {
        assert_eq!(
            multiply("9007199254740993", 3).unwrap().to_string(),
            "27021597764222979"
        );
        for s in ["01", "-1", "1.0", " 1", "", "1e9"] {
            assert!(atomic(s).is_err());
        }
        let m = (BigUint::from(1u32) << 256u32) - 1u32;
        assert!(add(m, BigUint::from(1u32)).is_err());
    }
}
