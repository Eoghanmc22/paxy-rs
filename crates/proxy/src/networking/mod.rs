pub mod utils;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use mio::{Events, Poll};

use packet_transformation::handling::{HandlingContext, UnparsedPacket};
use ::utils::contexts::{Message, NetworkThreadContext, ConnectionContext};
use ::utils::contexts::Message::{Threads, NewConnection};
use ::utils::indexed_vec::IndexedVec;
use self::utils::{write_socket0, unbuffer_read, write_socket, buffer_read, copy_slice_to, validate_small_frame, read_socket};
use ::utils::buffers::VarInts;
use ::utils::set_vec_len;

/// Start network thread loop.
/// Responsible for parsing and transforming every out/incoming packets.
pub fn thread_loop(rx: Receiver<Message>, handler: Arc<HandlingContext>, id: usize) {
    // Create thread context
    let mut thread_ctx = {
        let connections = HashMap::new();
        let threads = match rx.recv().unwrap() {
            Threads(threads) => {
                threads
            }
            _ => panic!("unexpected message")
        };

        let thread = threads[id].clone();

        NetworkThreadContext {
            connections,
            threads,
            thread,
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
    ::utils::set_vec_len(&mut packet_buf.vec, 2048);
    let mut uncompressed_buf = IndexedVec::new();
    ::utils::set_vec_len(&mut uncompressed_buf.vec, 2048);
    let mut caching_buf = IndexedVec::new();
    ::utils::set_vec_len(&mut caching_buf.vec, 2048);

    let mut id_counter = 0;

    // Start parsing loop
    loop {
        poll.poll(&mut events, Some(dur)).expect("couldn't poll");
        for event in events.iter() {
            // FIXME: I used remove to get around the borrow checker hopefully there is a better way. also i assume this is slower.
            if let Some(mut player) = thread_ctx.connections.remove(&event.token()) {
                if event.is_writable() {
                    process_write(&mut player);
                }
                if event.is_readable() {
                    let mut other = thread_ctx.connections.remove(&player.token_other).unwrap();
                    process_read(&mut thread_ctx, &mut player, &mut other, &mut packet_buf, &mut uncompressed_buf, &mut caching_buf, handler.clone());

                    thread_ctx.connections.insert(player.token_other.clone(), other);
                }

                if player.should_close {
                    // Connection socket is not active anymore, remove context
                    thread_ctx.connections.remove(&player.token_other);
                    continue;
                }

                thread_ctx.connections.insert(player.token_self.clone(), player);
            }
        }

        // Process all incoming messages
        for msg in rx.try_iter() {
            match msg {
                NewConnection(c2s, s2c) => {
                    // New connection has been associated to this thread
                    println!("Player connection");
                    // Create connection context
                    ConnectionContext::create_pair(id_counter, c2s, s2c, &poll, &mut thread_ctx.connections);
                    id_counter += 1;
                }
                _ => { println!("got unexpected message"); }
            }
        }
    }
}

// write buffered data
fn process_write(ctx: &mut ConnectionContext) {
    ctx.is_writable = true;
    if !write_socket0(&mut ctx.stream, &mut ctx.write_buffering, &mut ctx.should_close) {
        ctx.is_writable = false;
    }
}

// todo handle protocol state switching. right now we only check packet ids
// todo handle encryption
// todo handle compression
fn process_read(mut thread_ctx: &mut NetworkThreadContext,
                connection_ctx: &mut ConnectionContext,
                other_ctx: &mut ConnectionContext,
                read_buf: &mut IndexedVec<u8>,
                compression_buf: &mut IndexedVec<u8>,
                caching_buf: &mut IndexedVec<u8>,
                handler: Arc<HandlingContext>) {

    let mut pointer = 0;
    let mut next;
    read_buf.reset();
    compression_buf.reset();
    caching_buf.reset();

    // read new packets
    unbuffer_read(connection_ctx, read_buf);
    while read_socket(connection_ctx, read_buf) {
        if connection_ctx.should_close {
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
    if connection_ctx.should_close {
        return;
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
                    handler.handle_inbound_packet(&mut thread_ctx, connection_ctx, other_ctx, unparsed_packet)
                } else {
                    handler.handle_outbound_packet(&mut thread_ctx, connection_ctx, other_ctx, unparsed_packet)
                };

                if let Some(buffer) = optional_processed_buf {
                    // write in 2 steps to avoid extra copy
                    copy_slice_to(&read_buf.vec[pointer..pointer + (bytes + id_bytes) as usize], caching_buf);
                    copy_slice_to(&buffer.vec[buffer.get_reader_index()..buffer.get_writer_index()], caching_buf);
                } else {
                    copy_slice_to(&read_buf.vec[pointer..next], caching_buf);
                }

                if connection_ctx.should_close {
                    write_socket(connection_ctx, caching_buf);
                    return;
                }

                pointer = next;
                read_buf.set_reader_index(pointer);
            } else {
                break;
            }
        }
    }

    buffer_read(connection_ctx, read_buf);
    write_socket(other_ctx, caching_buf);
}
