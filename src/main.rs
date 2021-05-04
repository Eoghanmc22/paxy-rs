mod packets;
mod utils;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::packets::handling::{HandlingContext, UnparsedPacket};
use std::sync::Arc;
use crate::utils::VarInts;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proxy_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25566);
    let server_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25565);

    let listener = TcpListener::bind(proxy_address).await?;

    let mut handler_context = HandlingContext::new();/*
    handler_context.register_outbound_packet_supplier(|buf| {
        Box::new(EntityPositionPacket::read(buf))
    });
    handler_context.register_outbound_transformer(|packet: &mut EntityPositionPacket| {
        packet.delta_x = 0;
        packet.delta_y = 100;
    });*/
    let handler_context = Arc::new(handler_context);

    loop {
        let (client_socket, _) = listener.accept().await?;

        let handler_context = handler_context.clone();
        tokio::spawn(async move {
            // Connect to minecraft server
            let server_socket = match TcpStream::connect(server_address).await {
                Ok(stream) => stream,
                Err(error) => panic!("Error while connecting to server {}", error),
            };
            let (client_read, client_write) = client_socket.into_split();
            let (server_read, server_write) = server_socket.into_split();

            // read from client task
            tokio::spawn(forward_data(client_read, server_write, true, handler_context.clone()));

            // read from server task
            tokio::spawn(forward_data(server_read, client_write, false, handler_context.clone()));
        });
    }
}

async fn forward_data(mut from: OwnedReadHalf, mut to: OwnedWriteHalf, inbound: bool, handler: Arc<HandlingContext>) {
    let mut buf = Vec::new();
    buf.reserve(2048);
    unsafe {
        buf.set_len(buf.len()+2048);
    }

    let mut read = 0usize;

    loop {
        match from.read(&mut buf[read..]).await {
            Ok(n) if n == 0 => {
                return
            },
            Ok(n) => {
                let len = n + read;

                process(&mut buf, &mut read, len, &mut to, inbound, handler.clone()).await;

                while n + read >= buf.len() {
                    buf.reserve(buf.len());
                    unsafe {
                        buf.set_len(buf.len()*2);
                    }
                }
            }
            Err(e) if e.kind() != io::ErrorKind::WouldBlock => {
                println!("error {}", e);
            }
            _ => {},
        };
    }
}

// todo handle protocol state switching. right now we only check packet ids
// todo handle encryption
// todo handle compression
async fn process(mut buf: &mut Vec<u8>, read: &mut usize, len: usize, to: &mut OwnedWriteHalf, inbound: bool, handler: Arc<HandlingContext>) {
    let mut pointer = 0;
    let mut next;

    // read all the packets
    while len > pointer {
        if len - pointer < 3 && !validate_small_frame(&mut buf, pointer, len) {
            break;
        }
        // this does a copy which isnt to optimal. really we want to create a mutable view
        let mut working_buf = &buf[pointer..];

        if let Some((packet_len, bytes)) = working_buf.get_var_i32_limit(3) {
            next = packet_len as usize + pointer + bytes as usize;

            // the full packet is available
            if len >= next {
                let (id, id_bytes) = working_buf.get_var_i32();

                let unparsed_packet = UnparsedPacket::new(id, working_buf);
                let optional_processed_buf = if inbound {
                    handler.handle_inbound_packet(unparsed_packet)
                } else {
                    handler.handle_outbound_packet(unparsed_packet)
                };

                if let Some(buffer) = optional_processed_buf {
                    // write in 2 steps to avoid extra copy
                    if let Err(e) = to.write_all(&buf[pointer..pointer+(bytes+id_bytes) as usize]).await {
                        panic!("failed to write to socket; err = {:?}", e);
                    }

                    if let Err(e) = to.write_all(&buffer[0..(packet_len-id_bytes) as usize]).await {
                        panic!("failed to write to socket; err = {:?}", e);
                    }
                } else {
                    if let Err(e) = to.write_all(&buf[pointer..next as usize]).await {
                        panic!("failed to write to socket; err = {:?}", e);
                    }
                }
                pointer = next;
            } else {
                break;
            }
        }
    }
    if len > pointer {
        // buffer data for next tcp packet
        let extra_data_len = len-pointer;
        let extra_data = &buf[pointer..extra_data_len+pointer].to_vec();
        buf[..extra_data_len].copy_from_slice(extra_data);
        *read = extra_data_len;
    } else {
        *read = 0;
    }
}

fn validate_small_frame(buf: &mut Vec<u8>, pointer: usize, len: usize) -> bool {
    if len > pointer && len - pointer < 3 {
        for index in pointer..len {
            if buf[index] < 128 {
                return true;
            }
        }
    }
    false
}