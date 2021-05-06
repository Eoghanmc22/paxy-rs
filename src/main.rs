mod packets;
mod utils;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::packets::handling::{HandlingContext, UnparsedPacket};
use std::sync::Arc;
use crate::utils::VarInts;
use mio::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::sync::mpsc::{SyncSender, Receiver, SendError};
use std::{sync, thread};
use mio::{Poll, Events, Token, Interest};
use crate::Message::{Threads, NewConnection};
use std::collections::HashMap;
use std::time::Duration;
use std::io::{Read, ErrorKind, Write};
use bytes::{BufMut, Buf};
use bytes::buf::UninitSlice;

pub enum Message {
    Threads(Arc<Vec<Arc<PaxyThread>>>),
    NewConnection(TcpStream, TcpStream)
}

pub struct PaxyThread {
    thread: JoinHandle<()>,
    channel: SyncSender<Message>
}

impl PaxyThread {
    pub fn spawn(handler: Arc<HandlingContext>, id: usize) -> PaxyThread {
        // todo adjust? this prob isnt enough
        let (tx, rx) = sync::mpsc::sync_channel(1000);
        let thread = thread::spawn(move || {
            forward_data(rx, handler, id);
        });
        PaxyThread { thread, channel: tx }
    }

    pub fn notify(&self, msg: Message) -> Result<(), SendError<Message>> {
        self.channel.send(msg)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proxy_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25566);
    let server_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 25565);

    let mut listener = TcpListener::bind(proxy_address)?;

    let mut handler_context = HandlingContext::new();/*
    handler_context.register_outbound_packet_supplier(|buf| {
        Box::new(EntityPositionPacket::read(buf))
    });
    handler_context.register_outbound_transformer(|packet: &mut EntityPositionPacket| {
        packet.delta_x = 0;
        packet.delta_y = 100;
    });*/
    let handler_context = Arc::new(handler_context);

    let thread_count = num_cpus::get() * 2;
    let mut threads = Vec::with_capacity(thread_count);
    for thread in 0..thread_count {
        let paxy_thread = PaxyThread::spawn(handler_context.clone(), thread);
        threads.push(Arc::new(paxy_thread));
    }
    // Finalize the threads list
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
                let (client_socket, _) = listener.accept()?;
                threads[next_thread].notify(NewConnection(client_socket, TcpStream::connect(server_address)?))?;
                next_thread += 1;
                next_thread %= thread_count;
            }
        }
    }
}

pub struct ConnectionContext {
    pub token_self : Token,
    pub token_other : Token,
    pub stream : TcpStream,
    pub compression_threshold: i32,
    pub state: u8,
    pub should_close : bool,
    pub read_buffering: IndexedVec<u8>,
    pub write_buffering: IndexedVec<u8>,
    pub is_writable: bool,
    pub inbound: bool,
}

impl ConnectionContext {
    pub fn create_pair(id: usize, mut c2s: TcpStream, mut s2c: TcpStream, poll: &Poll, connections: &mut HashMap<Token, ConnectionContext>) {
        let c2s_token = Token(id*2);
        let s2c_token = Token(id*2+1);
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
            inbound: true
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
            inbound: false
        };
        connections.insert(c2s_token, c2s_context);
        connections.insert(s2c_token, s2c_context);
    }

    pub fn get_other<'a>(&self, thread_ctx: &'a mut NetworkThreadContext) -> &'a mut ConnectionContext {
        thread_ctx.connections.get_mut(&self.token_other).unwrap()
    }
}

pub struct NetworkThreadContext {
    pub connections: HashMap<Token, ConnectionContext>,
    pub threads: Arc<Vec<Arc<PaxyThread>>>,
    pub thread: Arc<PaxyThread>
}

pub fn set_vec_len<T>(vec: &mut Vec<T>, extra_len: usize) {
    vec.reserve(extra_len);

    // SAFETY:
    // This is safe because we will always write the uninitialized memory before reading it
    // and we reserve enough capacity.

    // Reason we dont use a safe method is because this is a lot faster. We dont need to initialize all of it
    // with 0s just to overwrite them.
    unsafe {
        vec.set_len(vec.len()+extra_len);
    }
}

fn forward_data(rx: Receiver<Message>, handler: Arc<HandlingContext>, id: usize) {
    let mut thread_ctx = {
        let connections = HashMap::new();
        let threads = match rx.recv().unwrap() {
            Threads(threads) => {
                threads
            },
            _ => panic!("unexpected message")
        };

        let thread = threads[id].clone();

        NetworkThreadContext {
            connections, threads, thread
        }
    };

    // todo adjust?
    let mut events = Events::with_capacity(1000);
    let mut poll = Poll::new().expect("could not unwrap poll");

    // max interval for polling the message queue
    // todo adjust?
    let dur = Duration::from_millis(10);

    //Per thread buffers
    let mut packet_buf = IndexedVec::new();
    set_vec_len(&mut packet_buf.vec, 2048);
    let mut uncompressed_buf = IndexedVec::new();
    set_vec_len(&mut uncompressed_buf.vec, 2048);

    let mut id_counter = 0;

    loop {
        poll.poll(&mut events, Some(dur)).expect("couldn't poll");
        for event in events.iter() {
            // I used remove to get around the borrow checker hopefully there is a better way. also i assume this is slower.
            if let Some(mut player) = thread_ctx.connections.remove(&event.token()) {
                if event.is_writable() {
                    process_write(&mut player);
                }
                if event.is_readable() {
                    let mut other = thread_ctx.connections.remove(&player.token_other).unwrap();
                    process_read(&thread_ctx, &mut player, &mut other, &mut packet_buf, &mut uncompressed_buf, handler.clone());

                    thread_ctx.connections.insert(player.token_other.clone(), other);
                }
                thread_ctx.connections.insert(player.token_self.clone(), player);
            }
        }
        for msg in rx.try_iter() {
            match msg {
                NewConnection(c2s, s2c) => {
                    // connect player
                    ConnectionContext::create_pair(id_counter, c2s, s2c, &poll, &mut thread_ctx.connections);
                    id_counter += 1;
                }
                _ => { println!("got unexpected message");}
            }
        }
    }
}

#[derive(Debug)]
pub struct IndexedVec<T> {
    pub vec: Vec<T>,
    writer_index: usize,
    reader_index: usize,
}

impl<T> IndexedVec<T> {
    pub fn new() -> IndexedVec<T> {
        IndexedVec {
            vec: Vec::new(),
            writer_index: 0,
            reader_index: 0
        }
    }

    pub fn from_vec(vec: Vec<T>) -> IndexedVec<T> {
        IndexedVec {
            vec,
            writer_index: 0,
            reader_index: 0
        }
    }

    pub fn get_writer_index(&self) -> usize {
        self.writer_index
    }

    pub fn get_reader_index(&self) -> usize {
        self.reader_index
    }

    pub fn set_writer_index(&mut self, writer_index: usize) {
        self.writer_index = writer_index;
    }

    pub fn set_reader_index(&mut self, writer_index: usize) {
        self.reader_index = writer_index;
    }

    pub fn advance_writer_index(&mut self, distance: usize) {
        self.writer_index += distance;
    }

    pub fn advance_reader_index(&mut self, distance: usize) {
        self.reader_index += distance;
    }

    pub fn reset(&mut self) {
        self.writer_index = 0;
        self.reader_index = 0;
    }

    pub fn reset_reader(&mut self) {
        self.reader_index = 0;
    }

    pub fn reset_writer(&mut self) {
        self.writer_index = 0;
    }

    pub fn ensure_writable(&mut self, extra: usize) {
        let remaining = self.vec.len() - self.get_writer_index();
        if remaining < extra {
            let needed = extra - remaining;
            set_vec_len(&mut self.vec, needed);
        }
    }
}

impl Buf for IndexedVec<u8> {
    fn remaining(&self) -> usize {
        self.vec.len() - self.get_reader_index()
    }

    fn chunk(&self) -> &[u8] {
        &self.vec[self.get_reader_index()..]
    }

    fn advance(&mut self, cnt: usize) {
        self.advance_reader_index(cnt);
        if self.get_reader_index() >= self.vec.len() {
            panic!("no more space, reader_index: {} cnt: {}, len: {}", self.get_reader_index()-cnt, cnt, self.vec.len())
        }
    }
}

unsafe impl BufMut for IndexedVec<u8> {
    fn remaining_mut(&self) -> usize {
        self.vec.len() - self.get_writer_index()
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.advance_writer_index(cnt);
        if self.get_writer_index() > self.vec.len() {
            panic!("no more space, writer_index: {} cnt: {}, len: {}", self.get_writer_index()-cnt, cnt, self.vec.len())
        }
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        if self.get_writer_index() == self.vec.len() {
            set_vec_len(&mut self.vec, 64) // Grow the vec
        }

        let cap = self.vec.len();
        let len = self.get_writer_index();

        let ptr = self.vec.as_mut_ptr();
        unsafe { &mut UninitSlice::from_raw_parts_mut(ptr, cap)[len..] }
    }
}

pub fn read_socket(ctx: &mut ConnectionContext, packet: &mut IndexedVec<u8>) -> bool {
    let w_i = packet.get_writer_index();
    let result = ctx.stream.read(&mut packet.vec[w_i as usize..]);
    match result {
        Ok(read) => {
            packet.advance_writer_index(read);
            true
        }
        Err(e) => {
            match e.kind() {
                ErrorKind::WouldBlock => {}
                _ => {
                    panic!("unable to read socket: {:?}", e)
                }
            }
            false
        }
    }
}

pub fn write_socket(ctx: &mut ConnectionContext, packet: &mut IndexedVec<u8>) {
    if ctx.is_writable {
        if !write_socket0(&mut ctx.stream, packet) {
            buffer_write(ctx, packet);
            ctx.is_writable = false;
        }
    } else {
        buffer_write(ctx, packet);
    }
}

pub fn write_socket_slice(ctx: &mut ConnectionContext, packet: &[u8]) {
    if ctx.is_writable {
        let mut total_written = 0;
        loop {
            let result = ctx.stream.write(&packet[total_written..]);
            match result {
                Ok(written) => {
                    total_written += written;
                }
                Err(e) => {
                    match e.kind() {
                        ErrorKind::WouldBlock => {
                            buffer_write_slice(ctx, packet, total_written);
                            ctx.is_writable = false;
                            break;
                        }
                        _ => {
                            panic!("unable to write socket: {:?}", e)
                        }
                    }
                }
            }
            if total_written >= packet.len() {
                break;
            }
        }
    } else {
        buffer_write_slice(ctx, packet, 0);
    }
}

fn write_socket0(stream: &mut TcpStream, packet: &mut IndexedVec<u8>) -> bool {
    loop {
        let range = packet.get_reader_index()..packet.get_writer_index();
        let result = stream.write(&mut packet.vec[range]);
        match result {
            Ok(written) => {
                packet.advance_reader_index(written);
            }
            Err(e) => {
                match e.kind() {
                    ErrorKind::WouldBlock => {
                        return false;
                    }
                    _ => {
                        panic!("unable to write socket: {:?}", e)
                    }
                }
            }
        }
        if packet.get_reader_index() >= packet.get_writer_index() {
            break;
        }
    }
    true
}

pub fn buffer_read(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &buffering_buf.vec[buffering_buf.get_reader_index()..buffering_buf.get_writer_index()];
    ctx.read_buffering.ensure_writable(slice.len());
    ctx.read_buffering.put_slice(slice);
}

pub fn unbuffer_read(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &ctx.read_buffering.vec[ctx.read_buffering.get_reader_index()..ctx.read_buffering.get_writer_index()];
    buffering_buf.ensure_writable(slice.len());
    buffering_buf.put_slice(slice);
    ctx.read_buffering.reset();
}

pub fn buffer_write(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    println!("buffer_write");
    let slice = &buffering_buf.vec[buffering_buf.get_reader_index()..buffering_buf.get_writer_index()];
    ctx.write_buffering.ensure_writable(slice.len());
    ctx.write_buffering.put_slice(slice);
}

pub fn buffer_write_slice(ctx: &mut ConnectionContext, buffering_buf: &[u8], start: usize) {
    println!("buffer_write_slice");
    let slice = &buffering_buf[start..];
    ctx.write_buffering.ensure_writable(slice.len());
    ctx.write_buffering.put_slice(slice);
}

pub fn unbuffer_write(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &ctx.write_buffering.vec[ctx.write_buffering.get_reader_index()..ctx.write_buffering.get_writer_index()];
    buffering_buf.put_slice(slice);
    ctx.write_buffering.reset();
}

// write buffered data
fn process_write(ctx: &mut ConnectionContext) {
    ctx.is_writable = true;
    if !write_socket0(&mut ctx.stream, &mut ctx.write_buffering) {
        ctx.is_writable = false;
    }
}

// todo handle protocol state switching. right now we only check packet ids
// todo handle encryption
// todo handle compression
fn process_read(thread_ctx: &NetworkThreadContext, connection_ctx: &mut ConnectionContext, other_ctx: &mut ConnectionContext, read_buf: &mut IndexedVec<u8>, compression_buf: &mut IndexedVec<u8>, handler: Arc<HandlingContext>) {
    let mut pointer = 0;
    let mut next;
    read_buf.reset();

    //read new packets
    unbuffer_read(connection_ctx, read_buf);
    while read_socket(connection_ctx, read_buf) {
        if read_buf.get_writer_index() == 0 {
            connection_ctx.should_close = true;
            return;
        }
        if read_buf.get_writer_index() == read_buf.vec.len() {
            let len = read_buf.vec.len();
            set_vec_len(&mut read_buf.vec, len);
            read_socket(connection_ctx, read_buf);
        } else {
            break;
        }
    }

    let len = read_buf.get_writer_index();

    // read all the packets
    while len > pointer {
        if len - pointer < 3 && !validate_small_frame(read_buf, pointer, len) {
            break;
        }
        let mut working_buf = &read_buf.vec[pointer..];

        if let Some((packet_len, bytes)) = working_buf.get_var_i32_limit(3) {
            next = packet_len as usize + pointer + bytes as usize;

            // the full packet is available
            if len >= next {
                let (id, id_bytes) = working_buf.get_var_i32();

                let unparsed_packet = UnparsedPacket::new(id, working_buf);
                let optional_processed_buf = if connection_ctx.inbound {
                    handler.handle_inbound_packet(thread_ctx, connection_ctx, unparsed_packet)
                } else {
                    handler.handle_outbound_packet(thread_ctx, connection_ctx, unparsed_packet)
                };

                if let Some(mut buffer) = optional_processed_buf {
                    // write in 2 steps to avoid extra copy
                    write_socket_slice(other_ctx, &read_buf.vec[pointer..pointer+(bytes+id_bytes) as usize]);
                    write_socket(other_ctx, &mut buffer);
                } else {
                    write_socket_slice(other_ctx, &read_buf.vec[pointer..next]);
                }

                pointer = next;
                read_buf.set_reader_index(pointer);
            } else {
                break;
            }
        }
    }

    buffer_read(connection_ctx, read_buf);
}

fn validate_small_frame(buf: &mut IndexedVec<u8>, pointer: usize, len: usize) -> bool {
    if len > pointer && len - pointer < 3 {
        for index in pointer..len {
            if buf.vec[index] < 128 {
                return true;
            }
        }
    }
    false
}