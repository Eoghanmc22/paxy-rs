use std::{sync, thread};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{SendError, SyncSender};
use std::thread::JoinHandle;

use mio::{Interest, Poll, Token};
use mio::net::TcpStream;

use crate::packets::handling::HandlingContext;
use crate::utils::IndexedVec;

pub struct PaxyThread {
    thread: JoinHandle<()>,
    channel: SyncSender<Message>,
}

impl PaxyThread {
    pub fn spawn(handler: Arc<HandlingContext>, id: usize) -> PaxyThread {
        // todo adjust? this prob isnt enough
        let (tx, rx) = sync::mpsc::sync_channel(1000);
        let thread = thread::spawn(move || {
            crate::packets::networking::forward_data(rx, handler, id);
        });
        PaxyThread { thread, channel: tx }
    }

    pub fn notify(&self, msg: Message) -> Result<(), SendError<Message>> {
        self.channel.send(msg)
    }
}

/// Context linked to a TCP connection.
/// Used to store its state, and ensure that packets are parsed the right way.
pub struct ConnectionContext {
    pub token_self: Token,
    pub token_other: Token,
    pub stream: TcpStream,
    pub compression_threshold: i32,
    pub state: u8,
    pub should_close: bool,
    pub read_buffering: IndexedVec<u8>,
    pub write_buffering: IndexedVec<u8>,
    pub is_writable: bool,
    pub inbound: bool,
}

impl ConnectionContext {
    pub fn create_pair(id: usize, mut c2s: TcpStream, mut s2c: TcpStream, poll: &Poll, connections: &mut HashMap<Token, ConnectionContext>) {
        let c2s_token = Token(id * 2);
        let s2c_token = Token(id * 2 + 1);
        let registry = poll.registry();
        registry.register(&mut c2s, c2s_token, Interest::READABLE | Interest::WRITABLE).unwrap();
        registry.register(&mut s2c, s2c_token, Interest::READABLE | Interest::WRITABLE).unwrap();
        let c2s_context = ConnectionContext {
            token_self: c2s_token,
            token_other: s2c_token,
            stream: c2s,
            compression_threshold: 0,
            state: 0,
            should_close: false,
            read_buffering: IndexedVec::new(),
            write_buffering: IndexedVec::new(),
            is_writable: true,
            inbound: true,
        };
        let s2c_context = ConnectionContext {
            token_self: s2c_token,
            token_other: c2s_token,
            stream: s2c,
            compression_threshold: 0,
            state: 0,
            should_close: false,
            read_buffering: IndexedVec::new(),
            write_buffering: IndexedVec::new(),
            is_writable: true,
            inbound: false,
        };
        connections.insert(c2s_token, c2s_context);
        connections.insert(s2c_token, s2c_context);
    }

    pub fn get_other<'a>(&self, thread_ctx: &'a mut NetworkThreadContext) -> &'a mut ConnectionContext {
        thread_ctx.connections.get_mut(&self.token_other).unwrap()
    }
}

/// Context linked to a single networking thread.
/// Stores all the connections and their tokens
pub struct NetworkThreadContext {
    pub connections: HashMap<Token, ConnectionContext>,
    pub threads: Arc<Vec<Arc<PaxyThread>>>,
    pub thread: Arc<PaxyThread>,
}

/// Represents a message sent to a [`NetworkThreadContext`].
pub enum Message {
    Threads(Arc<Vec<Arc<PaxyThread>>>),

    /// New connection should be processed in the receiving thread.
    NewConnection(TcpStream, TcpStream),
}
