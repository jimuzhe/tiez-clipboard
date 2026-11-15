//! The machine's network interfaces and its local address.
//!
//! On macOS `local_ip_address` never frees the list `getifaddrs` returns, and
//! network recovery reads it every few seconds, so memory grew for as long as
//! TieZ ran. macOS reads the list here and frees it; other platforms keep the
//! crate, which frees what it allocates there.

use std::net::IpAddr;

/// Every IPv4 and IPv6 address, with the name of its interface.
#[cfg(target_os = "macos")]
pub fn list_afinet_netifas() -> Option<Vec<(String, IpAddr)>> {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return None;
    }
    let mut interfaces = Vec::new();
    let mut ifa = head;
    while !ifa.is_null() {
        let entry = unsafe { &*ifa };
        if let Some(ip) = unsafe { address(entry.ifa_addr) } {
            let name = unsafe { std::ffi::CStr::from_ptr(entry.ifa_name) };
            interfaces.push((name.to_string_lossy().into_owned(), ip));
        }
        ifa = entry.ifa_next;
    }
    unsafe { libc::freeifaddrs(head) };
    Some(interfaces)
}

#[cfg(target_os = "macos")]
unsafe fn address(addr: *const libc::sockaddr) -> Option<IpAddr> {
    if addr.is_null() {
        return None;
    }
    match (*addr).sa_family as i32 {
        libc::AF_INET => {
            let v4 = &*(addr as *const libc::sockaddr_in);
            // s_addr holds the address bytes in network order.
            Some(IpAddr::from(v4.sin_addr.s_addr.to_ne_bytes()))
        }
        libc::AF_INET6 => {
            let v6 = &*(addr as *const libc::sockaddr_in6);
            Some(IpAddr::from(v6.sin6_addr.s6_addr))
        }
        _ => None,
    }
}

/// The first IPv4 address that isn't loopback, as `local_ip_address` picks it.
#[cfg(target_os = "macos")]
pub fn local_ip() -> Option<IpAddr> {
    list_afinet_netifas()?
        .into_iter()
        .map(|(_, ip)| ip)
        .find(|ip| ip.is_ipv4() && !ip.is_loopback())
}

#[cfg(not(target_os = "macos"))]
pub fn list_afinet_netifas() -> Option<Vec<(String, IpAddr)>> {
    local_ip_address::list_afinet_netifas().ok()
}

#[cfg(not(target_os = "macos"))]
pub fn local_ip() -> Option<IpAddr> {
    local_ip_address::local_ip().ok()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{list_afinet_netifas, local_ip};

    #[test]
    fn interfaces_match_what_local_ip_address_reported() {
        let mut ours = list_afinet_netifas().unwrap();
        let mut theirs = local_ip_address::list_afinet_netifas().unwrap();
        ours.sort();
        theirs.sort();
        assert_eq!(ours, theirs);
        assert!(ours.iter().any(|(_, ip)| ip.is_loopback()));
        assert_eq!(local_ip(), local_ip_address::local_ip().ok());
    }
}
