//! Proxy headers have no authority unless the socket peer is explicitly trusted.
use serde::{Deserialize, Deserializer};
use std::{net::IpAddr, str::FromStr};
#[derive(Clone, Debug)]
pub struct Cidr {
    network: IpAddr,
    prefix: u32,
}
impl FromStr for Cidr {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ip, prefix) = s
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("CIDR prefix required"))?;
        let network: IpAddr = ip.parse()?;
        let prefix: u32 = prefix.parse()?;
        anyhow::ensure!(
            prefix <= if network.is_ipv4() { 32 } else { 128 },
            "invalid CIDR prefix"
        );
        Ok(Self { network, prefix })
    }
}
impl<'de> Deserialize<'de> for Cidr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
impl Cidr {
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(a), IpAddr::V4(b)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix)
                };
                u32::from(a) & mask == u32::from(b) & mask
            }
            (IpAddr::V6(a), IpAddr::V6(b)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix)
                };
                u128::from(a) & mask == u128::from(b) & mask
            }
            _ => false,
        }
    }
}
pub fn client_ip(peer: IpAddr, headers: &axum::http::HeaderMap, trusted: &[Cidr]) -> IpAddr {
    let is_trusted = |ip| trusted.iter().any(|c| c.contains(ip));
    if !is_trusted(peer) {
        return peer;
    }
    let values = headers.get_all("x-forwarded-for");
    let mut chain = Vec::new();
    for value in values {
        let Ok(value) = value.to_str() else {
            return peer;
        };
        for item in value.split(',') {
            let Ok(ip) = item.trim().parse::<IpAddr>() else {
                return peer;
            };
            chain.push(ip);
            if chain.len() > 32 {
                return peer;
            }
        }
    }
    let mut current = peer;
    for ip in chain.into_iter().rev() {
        if !is_trusted(current) {
            break;
        }
        current = ip;
    }
    current
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundaries() {
        for (cidr, inside, outside) in [
            ("192.0.2.128/25", "192.0.2.255", "192.0.2.127"),
            ("2001:db8:8000::/33", "2001:db8:ffff::1", "2001:db8:7fff::1"),
            ("127.0.0.1/32", "127.0.0.1", "127.0.0.2"),
            ("::1/128", "::1", "::2"),
        ] {
            let c: Cidr = cidr.parse().unwrap();
            assert!(c.contains(inside.parse().unwrap()));
            assert!(!c.contains(outside.parse().unwrap()));
        }
        assert!("::/129".parse::<Cidr>().is_err());
        assert!("1.2.3.4/33".parse::<Cidr>().is_err());
        assert!(
            "0.0.0.0/0"
                .parse::<Cidr>()
                .unwrap()
                .contains("8.8.8.8".parse().unwrap())
        );
    }
    #[test]
    fn spoof_and_chain() {
        let peer = "10.0.0.1".parse().unwrap();
        let mut h = axum::http::HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            "1.1.1.1, 198.51.100.2, 10.0.0.2".parse().unwrap(),
        );
        assert_eq!(client_ip(peer, &h, &[]), peer);
        let trusted = vec!["10.0.0.0/24".parse().unwrap()];
        assert_eq!(
            client_ip(peer, &h, &trusted),
            "198.51.100.2".parse::<IpAddr>().unwrap()
        );
        h.insert("x-forwarded-for", "bad, 10.0.0.2".parse().unwrap());
        assert_eq!(client_ip(peer, &h, &trusted), peer);
    }
}
