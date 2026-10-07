//! UDP sockets for media: which local addresses to offer, binding within the configured port
//! range, and the reader tasks that hand datagrams to a room.

use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket as StdUdpSocket};
use std::sync::Arc;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Large enough for any datagram a browser sends over WebRTC.
const RECEIVE_BUFFER: usize = 2_048;

/// One datagram that arrived on one of a room's sockets.
#[derive(Debug)]
pub(crate) struct Datagram {
    /// The socket's own address (the ICE candidate's base).
    pub(crate) local: SocketAddr,
    pub(crate) source: SocketAddr,
    pub(crate) data: Vec<u8>,
    pub(crate) at: Instant,
}

/// The interface addresses to gather host candidates on.
///
/// IPv4 only unless asked: the port forward and `WEBRTC_SFU_PUBLIC_IP` are IPv4, browsers hide
/// their IPv6 host addresses behind mDNS names, and a LAN browser that picks the server's IPv6
/// candidate has been seen to lose its media on it. Loopback and link-local addresses are
/// skipped unless they are all there is (a host without a network still serves itself).
pub(crate) fn host_addresses(ipv6: bool) -> Vec<IpAddr> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_default();
    select_hosts(interfaces.iter().map(if_addrs::Interface::ip), ipv6)
}

/// [`host_addresses`] over a given list of interface addresses.
fn select_hosts(interfaces: impl IntoIterator<Item = IpAddr>, ipv6: bool) -> Vec<IpAddr> {
    let usable: Vec<IpAddr> = interfaces
        .into_iter()
        .filter(|ip| ipv6 || ip.is_ipv4())
        .collect();
    let mut addresses: Vec<IpAddr> = usable
        .iter()
        .copied()
        .filter(|ip| {
            !ip.is_loopback() && !link_local(ip) && !ip.is_unspecified() && !ip.is_multicast()
        })
        .collect();
    if addresses.is_empty() {
        addresses = usable.into_iter().filter(IpAddr::is_loopback).collect();
    }
    addresses.sort();
    addresses.dedup();
    addresses
}

fn link_local(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_unicast_link_local(),
    }
}

/// A socket on `ip` with the first free port in `port_min..=port_max`.
pub(crate) fn bind_in_range(ip: IpAddr, port_min: u16, port_max: u16) -> io::Result<UdpSocket> {
    let mut last_error = io::Error::new(io::ErrorKind::AddrInUse, "empty UDP port range");
    for port in port_min..=port_max {
        match StdUdpSocket::bind(SocketAddr::new(ip, port)) {
            Ok(socket) => {
                socket.set_nonblocking(true)?;
                return UdpSocket::from_std(socket);
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

/// A bound socket and the task reading it.
#[derive(Debug)]
pub(crate) struct ReadSocket {
    pub(crate) local: SocketAddr,
    pub(crate) socket: Arc<UdpSocket>,
    reader: JoinHandle<()>,
}

impl ReadSocket {
    /// Starts reading `socket` into `datagrams`. When the room is too far behind to take a
    /// datagram it is dropped, as the network would.
    pub(crate) fn start(socket: UdpSocket, datagrams: mpsc::Sender<Datagram>) -> io::Result<Self> {
        let local = socket.local_addr()?;
        let socket = Arc::new(socket);
        let reading = Arc::clone(&socket);
        let reader = tokio::spawn(async move {
            let mut buffer = vec![0; RECEIVE_BUFFER];
            loop {
                match reading.recv_from(&mut buffer).await {
                    Ok((length, source)) => {
                        let data = buffer.get(..length).unwrap_or_default().to_vec();
                        let datagram = Datagram {
                            local,
                            source,
                            data,
                            at: Instant::now(),
                        };
                        match datagrams.try_send(datagram) {
                            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                            Err(mpsc::error::TrySendError::Closed(_)) => break,
                        }
                    }
                    // ICMP errors (port unreachable) surface as receive errors on some
                    // platforms; they say nothing about this socket.
                    Err(error) => tracing::debug!("SFU socket {local} receive error: {error}"),
                }
            }
        });
        Ok(Self {
            local,
            socket,
            reader,
        })
    }

    /// Sends without waiting; a full socket buffer drops the datagram, as the network would.
    pub(crate) fn send(&self, destination: SocketAddr, data: &[u8]) {
        if let Err(error) = self.socket.try_send_to(data, destination) {
            tracing::debug!(
                "SFU socket {} could not send to {destination}: {error}",
                self.local
            );
        }
    }
}

impl Drop for ReadSocket {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(list: &[&str]) -> Vec<IpAddr> {
        list.iter().map(|ip| ip.parse().unwrap()).collect()
    }

    /// A container whose service starts before DHCP has finished sees only loopback (and an
    /// IPv6 link-local address); a moment later the interface has its IPv4 address.
    #[test]
    fn loopback_is_the_fallback_until_an_interface_has_an_address() {
        let booting = ips(&["127.0.0.1", "::1", "fe80::1"]);
        assert_eq!(select_hosts(booting, false), ips(&["127.0.0.1"]));
        let up = ips(&["127.0.0.1", "::1", "fe80::1", "192.168.1.50", "169.254.3.4"]);
        assert_eq!(select_hosts(up.clone(), false), ips(&["192.168.1.50"]));
        assert_eq!(select_hosts(up, true), ips(&["192.168.1.50"]));
        assert_eq!(
            select_hosts(ips(&["10.0.0.5", "2001:db8::5", "10.0.0.5"]), true),
            ips(&["10.0.0.5", "2001:db8::5"])
        );
        assert!(select_hosts(Vec::new(), false).is_empty());
    }
}
