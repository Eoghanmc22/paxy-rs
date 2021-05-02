use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::io;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proxy_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25566);
    let server_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25565);

    let listener = TcpListener::bind(proxy_address).await?;

    loop {
        let (mut client_socket, _) = listener.accept().await?;

        tokio::spawn(async move {

            // Connect to minecraft server
            let mut server_socket = match TcpStream::connect(server_address).await {
                Ok(stream) => stream,
                Err(error) => panic!("Error while connecting to server {}", error),
            };

            let mut buf = [0; 65535];

            // In a loop, read data from the socket and write the data back.
            loop {
                // Read client
                match client_socket.try_read(&mut buf) {
                    Ok(n) if n == 0 => return,
                    Ok(n) => {
                        if let Err(e) = server_socket.write_all(&buf[0..n]).await {
                            eprintln!("failed to write to socket; err = {:?}", e);
                            return;
                        }
                    }
                    Err(ref e) if e.kind() != io::ErrorKind::WouldBlock => {
                        println!("error {}",e);
                    }
                    _=>(),
                };

                // Read server
                match server_socket.try_read(&mut buf) {
                    Ok(n) if n == 0 => return,
                    Ok(n) => {
                        if let Err(e) = client_socket.write_all(&buf[0..n]).await {
                            eprintln!("failed to write to socket; err = {:?}", e);
                            return;
                        }
                    }
                    Err(ref e) if e.kind() != io::ErrorKind::WouldBlock => {
                        println!("error {}",e);
                    }
                    _=>(),
                };
            }
        });
    }
}