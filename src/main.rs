use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use mio::{Events, Interest, Poll, Token};
use mio::net::{TcpListener, TcpStream};

use contexts::PaxyThread;
use contexts::Message::{NewConnection, Threads};

use crate::packets::handling::HandlingContext;
use crate::packets::{Packet, networking};
use crate::packets::s2c::play::EntityPositionPacket;
use crate::packets::c2s::handshake::HandshakePacket;
use crate::packets::s2c::login::LoginSuccess;

mod packets;
mod utils;
mod contexts;

fn register_packets(handler_context: &mut HandlingContext) {
    handler_context.register_packet_supplier(|buf| {
        EntityPositionPacket::read(buf)
    });

    handler_context.register_packet_supplier(|buf| {
        HandshakePacket::read(buf)
    });
    handler_context.register_transformer(|mut thread_ctx, connection_ctx, other_ctx, packet: &mut HandshakePacket| {
        connection_ctx.state = packet.next_state;
        other_ctx.state = packet.next_state;
    });

    handler_context.register_packet_supplier(|buf| {
        LoginSuccess::read(buf)
    });
    handler_context.register_transformer(|mut thread_ctx, connection_ctx, other_ctx, _packet: &mut LoginSuccess| {
        connection_ctx.state = networking::PLAY_STATE;
        other_ctx.state = networking::PLAY_STATE;
    });
}

fn register_transformers(handler_context: &mut HandlingContext) {
    handler_context.register_transformer(|_thread_ctx, _connection_ctx, _other_ctx, packet: &mut EntityPositionPacket| {
        packet.delta_x = 0;
        packet.delta_y = 100;
    });
}

// TODO add transformer results like Updated, Unchanged, Cancelled
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proxy_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25566);
    let server_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25565);

    // Create TCP server
    let mut listener = TcpListener::bind(proxy_address)?;

    // Registering
    let mut handler_context = HandlingContext::new();
    register_packets(&mut handler_context);
    register_transformers(&mut handler_context);
    let handler_context = Arc::new(handler_context);

    // Setup network threads
    let thread_count = num_cpus::get() * 2;
    let mut threads = Vec::with_capacity(thread_count);
    for thread in 0..thread_count {
        let paxy_thread = PaxyThread::spawn(handler_context.clone(), thread);
        threads.push(Arc::new(paxy_thread));
    }
    // Finalize the thread list
    let threads = Arc::new(threads);

    for thread in threads.iter() {
        thread.notify(Threads(threads.clone()))?
    }

    let mut next_thread = 0usize;

    let mut events = Events::with_capacity(128);
    let mut poll = Poll::new().expect("could not unwrap poll");

    let listener_token = Token(0);
    poll.registry().register(&mut listener, listener_token, Interest::READABLE).unwrap();

    // handles accepting connections and messages a thread about it
    loop {
        poll.poll(&mut events, None).expect("couldn't poll");
        for event in events.iter() {
            if event.token() == listener_token {
                loop {
                    if let Ok((client_socket, _)) = listener.accept() {
                        // New client, bind it to a thread
                        threads[next_thread].notify(NewConnection(client_socket, TcpStream::connect(server_address)?))?;
                        next_thread += 1;
                        next_thread %= thread_count;
                    } else {
                        break;
                    }
                }
            }
        }
    }
}