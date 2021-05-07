use std::net::{SocketAddr, Ipv4Addr, IpAddr};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proxy_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25566);
    let server_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25565);
    proxy::start(proxy_address, server_address)
}