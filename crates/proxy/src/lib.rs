use std::net::SocketAddr;
use std::sync::Arc;

use mio::{Events, Interest, Poll, Token};
use mio::net::{TcpListener, TcpStream};

use utils::contexts::Message::{Threads, NewConnection};
use utils::contexts::PaxyThread;
use packet_transformation::handling::HandlingContext;
use packets::{c2s, s2c};
use std::{sync, thread};
use packet_transformation::TransformationResult::{Unchanged, Modified, Canceled};

mod networking;

fn register_packets(handler_context: &mut HandlingContext) {
    handler_context.register_transformer(|_thread_ctx, connection_ctx, other_ctx, packet: &mut c2s::handshake::HandshakePacket| {
        connection_ctx.state = packet.next_state.val as u8;
        other_ctx.state = packet.next_state.val as u8;
        Unchanged
    });
    handler_context.register_transformer(|_thread_ctx, connection_ctx, other_ctx, packet: &mut s2c::login::LoginSuccess| {
        connection_ctx.state = packets::PLAY_STATE;
        other_ctx.state = packets::PLAY_STATE;
        Unchanged
    });
    handler_context.register_transformer(|_thread_ctx, connection_ctx, other_ctx, packet: &mut s2c::login::SetCompression| {
        connection_ctx.compression_threshold = packet.threshold.val;
        other_ctx.compression_threshold = packet.threshold.val;
        Unchanged
    });
}

fn register_transformers(handler_context: &mut HandlingContext) {
    handler_context.register_transformer(|_thread_ctx, _connection_ctx, _other_ctx, packet: &mut s2c::play::EntityPositionPacket| {
        packet.delta_x = 0;
        packet.delta_y = 100;
        Modified
    });
    /*handler_context.register_transformer(|_thread_ctx, _connection_ctx, _other_ctx, _packet: &mut c2s::status::Ping| {
        Canceled
    });*/
}

fn spawn_thread(handler: Arc<HandlingContext>, id: usize) -> PaxyThread {
    // todo adjust? this prob isnt enough
    let (tx, rx) = sync::mpsc::sync_channel(1000);
    let thread = thread::spawn(move || {
        networking::thread_loop(rx, handler, id);
    });
    PaxyThread { thread, channel: tx }
}
// TODO add transformer results like Updated, Unchanged, Cancelled
pub fn start(proxy_address: SocketAddr, server_address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
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
        let paxy_thread = spawn_thread(handler_context.clone(), thread);
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