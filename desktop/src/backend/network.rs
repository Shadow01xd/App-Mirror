use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// "USB" when the peer is reached through a local interface other than the routed Wi-Fi one
/// (a phone sharing its connection over the cable shows up as a separate RNDIS network), else
/// "Wi-Fi". Heuristic: same /24 as that interface, different /24 from the routed address.
pub fn link_label(peer: IpAddr) -> &'static str {
    let IpAddr::V4(peer) = peer else {
        return "Wi-Fi";
    };
    let same_subnet = |a: Ipv4Addr, b: Ipv4Addr| a.octets()[..3] == b.octets()[..3];
    if let IpAddr::V4(routed) = pairing_ip()
        && same_subnet(routed, peer)
    {
        return "Wi-Fi";
    }
    let direct = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .any(|i| matches!(i.ip(), IpAddr::V4(local) if same_subnet(local, peer)));
    if direct { "USB" } else { "Wi-Fi" }
}

/// Ask the OS routing table for its preferred IPv4 source address. UDP connect
/// only sets a destination locally: no payload or Internet request is sent.
pub fn pairing_ip() -> IpAddr {
    if let Ok(ip) = std::env::var("TYU_ADVERTISE_IP")
        && let Ok(ip) = ip.parse()
    {
        return ip;
    }
    let addresses = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|interface| interface.ip())
        .collect::<Vec<_>>();
    let routed = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .ok()
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
            socket.local_addr().ok().map(|address| address.ip())
        });
    select_ip(&addresses, routed)
}

fn select_ip(addresses: &[IpAddr], routed: Option<IpAddr>) -> IpAddr {
    let eligible = |ip: &&IpAddr| matches!(ip, IpAddr::V4(v) if !v.is_loopback() && !v.is_link_local() && !v.is_unspecified());
    routed
        .filter(|ip| addresses.contains(ip) && eligible(&ip))
        .or_else(|| addresses.iter().find(eligible).copied())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qr_prefers_routed_wifi_over_first_virtual_interface() {
        let virtual_ip = "192.168.56.1".parse().unwrap();
        let wifi = "192.168.1.195".parse().unwrap();
        assert_eq!(select_ip(&[virtual_ip, wifi], Some(wifi)), wifi);
        assert_eq!(select_ip(&[virtual_ip], None), virtual_ip);
        assert_eq!(select_ip(&[], None), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
}
