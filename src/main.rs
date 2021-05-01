use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = TcpListener::bind("127.0.0.1:25566").await?;

    loop {
        let (mut client_socket, _) = listener.accept().await?;

        tokio::spawn(async move {

            // Connect to minecraft server
            let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25565);
            let mut server_socket = match TcpStream::connect(address).await {
                Ok(stream) => stream,
                Err(error) => panic!("Error while connecting to server"),
            };

            let mut buf = [0; 1024];

            // In a loop, read data from the socket and write the data back.
            loop {
                // Read client
                let client_size = match client_socket.try_read(&mut buf) {
                    Ok(n) if n == 0 => return,
                    Ok(n) => n,
                    _ => 0,
                };

                // Send to server
                write(&mut server_socket, &buf[0..client_size]).await;

                // Read server
                let server_size = match server_socket.try_read(&mut buf) {
                    Ok(n) if n == 0 => return,
                    Ok(n) => n,
                    _ => 0,
                };

                // Send to client
                write(&mut client_socket, &buf[0..server_size]).await;
            }
        });
    }
}

async fn write(socket: &mut (impl AsyncWriteExt + Unpin), buffer: &[u8]) {
    if let Err(e) = socket.write_all(buffer).await {
        eprintln!("failed to write to socket; err = {:?}", e);
        return;
    }
}

async fn test(stream: TcpStream) {
    match stream.peer_addr() {
        Ok(address) => println!("test {}", address),
        Err(error) => println!("error {}", error),
    }
}