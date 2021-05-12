use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{SendError, SyncSender};
use std::thread::JoinHandle;

use mio::{Interest, Poll, Token};
use mio::net::TcpStream;

use crate::indexed_vec::IndexedVec;
use crate::{Packet, get_var_i32_size};
use crate::buffers::VarIntsMut;
use crate::buffer_helpers::{compress_packet, write_socket};
use libdeflater::{Compressor, CompressionLvl};
use bytes::BufMut;

pub struct PaxyThread {
    pub thread: JoinHandle<()>,
    pub channel: SyncSender<Message>,
}

impl PaxyThread {
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

    pub fn send_packet<P: Packet>(&mut self, packet: &P) {
        let compression_threshold = self.compression_threshold;
        let mut buf = IndexedVec::new();
        // total len
        buf.advance_writer_index(3);
        buf.advance_reader_index(3);
        if compression_threshold > 0 {
            // the 0 if the packet is uncompressed
            buf.put_u8(0);
            buf.advance_reader_index(1);
        }
        buf.put_var_i32(P::get_id());
        packet.write(&mut buf);

        if compression_threshold > 0 {
            let mut buffer = buf.as_slice();
            if buffer.len() as i32 > compression_threshold {
                let mut compression_buf = IndexedVec::new();
                // total len
                compression_buf.advance_writer_index(3);
                compression_buf.advance_reader_index(3);
                compress_packet(&mut buffer, &mut Compressor::new(CompressionLvl::fastest()), &mut compression_buf);
                buf = compression_buf;
            }
        }

        let len = buf.get_writer_index()-3;

        let len_size = get_var_i32_size(len as i32) as usize;

        if len_size > 3 {
            println!("illegal packet len");
            self.should_close = true;
            return;
        }

        let start = 3 - len_size;
        let end = buf.get_writer_index();
        buf.set_writer_index(start);

        buf.put_var_i32(len as i32);

        buf.set_reader_index(start);
        buf.set_writer_index(end);

        write_socket(self, &mut buf);
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