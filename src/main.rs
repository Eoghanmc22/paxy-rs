mod packets;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proxy_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25566);
    let server_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25565);

    let listener = TcpListener::bind(proxy_address).await?;

    loop {
        let (client_socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            // Connect to minecraft server
            let server_socket = match TcpStream::connect(server_address).await {
                Ok(stream) => stream,
                Err(error) => panic!("Error while connecting to server {}", error),
            };
            let (client_read, client_write) = client_socket.into_split();
            let (server_read, server_write) = server_socket.into_split();

            // read from client task
            tokio::spawn(forward_data(client_read, server_write));

            // read from server task
            tokio::spawn(forward_data(server_read, client_write));
        });
    }

    async fn forward_data(mut from: OwnedReadHalf, mut to: OwnedWriteHalf) {
        let mut buf = [0; 65535];
        loop {
            match from.read(&mut buf).await {
                Ok(n) if n == 0 => return,
                Ok(n) => {
                    // TODO parse packet
                    if let Err(e) = to.write_all(&buf[0..n]).await {
                        eprintln!("failed to write to socket; err = {:?}", e);
                        return;
                    }
                }
                Err(e) if e.kind() != io::ErrorKind::WouldBlock => {
                    println!("error {}", e);
                }
                _ => (),
            };
        }
    }
}