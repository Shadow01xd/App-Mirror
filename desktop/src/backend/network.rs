use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// How a connected peer is reached. A cable link goes through the local ADB tunnel, so the
/// phone answers from loopback; anything else came over the network.
pub fn link_label(peer: IpAddr) -> &'static str {
    if peer.is_loopback() { "USB" } else { "Wi-Fi" }
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
    /// The cable tunnel terminates on loopback; a phone on the network never does.
    #[test]
    fn a_tunnelled_phone_is_labelled_usb() {
        assert_eq!(link_label("127.0.0.1".parse().unwrap()), "USB");
        assert_eq!(link_label("192.168.1.42".parse().unwrap()), "Wi-Fi");
        assert_eq!(link_label("10.0.0.7".parse().unwrap()), "Wi-Fi");
    }

    #[test]
    fn qr_prefers_routed_wifi_over_first_virtual_interface() {
        let virtual_ip = "192.168.56.1".parse().unwrap();
        let wifi = "192.168.1.195".parse().unwrap();
        assert_eq!(select_ip(&[virtual_ip, wifi], Some(wifi)), wifi);
        assert_eq!(select_ip(&[virtual_ip], None), virtual_ip);
        assert_eq!(select_ip(&[], None), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
}
