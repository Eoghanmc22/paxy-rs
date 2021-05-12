use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use libdeflater::{CompressionLvl, Compressor, Decompressor};
use mio::{Events, Poll};

use buffer_helpers::{buffer_read, copy_slice_to, read_frame, write_socket, write_socket0};
use packet_transformation::handling::{HandlingContext, UnparsedPacket};
use packet_transformation::TransformationResult;
use utils::buffers::{VarInts, VarIntsMut};
use utils::contexts::{ConnectionContext, Message, NetworkThreadContext};
use utils::contexts::Message::{NewConnection, Threads};
use utils::indexed_vec::IndexedVec;
use crate::networking::buffer_helpers::{get_needed_data, decompress_packet, compress_packet};

pub mod buffer_helpers;

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
    utils::set_vec_len(&mut packet_buf.vec, 2048);
    let mut compression_buf = IndexedVec::new();
    utils::set_vec_len(&mut compression_buf.vec, 2048);
    let mut caching_buf = IndexedVec::new();
    utils::set_vec_len(&mut caching_buf.vec, 2048);

    let mut decompressor = Decompressor::new();
    let mut compressor = Compressor::new(CompressionLvl::fastest());

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
                    process_read(&mut thread_ctx, &mut player, &mut other, &mut packet_buf, &mut caching_buf, handler.clone(), &mut compression_buf, &mut decompressor, &mut compressor);

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
                caching_buf: &mut IndexedVec<u8>,
                handler: Arc<HandlingContext>,
                compression_buffer: &mut IndexedVec<u8>,
                decompressor: &mut Decompressor,
                compressor: &mut Compressor) {

    let mut pointer = 0;
    let mut next;
    read_buf.reset();
    caching_buf.reset();

    // read new packets
    get_needed_data(read_buf, connection_ctx);
    if connection_ctx.should_close {
        return;
    }

    let readable = read_buf.readable_bytes();

    // read all the packets
    while readable > pointer {
        if let Some((packet_len, packet_len_bytes_red)) = read_frame(read_buf, pointer, readable, connection_ctx) {
            let offset = pointer + packet_len_bytes_red;
            next = offset + packet_len as usize;

            // the full packet is available
            if readable >= next {
                let mut working_buf = &read_buf.vec[offset..offset + packet_len];

                let compression_threshold = connection_ctx.compression_threshold;

                if compression_threshold > 0 {
                    let real_length = working_buf.get_var_i32();
                    if real_length.0 > 0 {
                        decompress_packet(real_length.0 as usize, &mut working_buf, decompressor, compression_buffer);
                    }
                }

                let (id, _id_bytes) = working_buf.get_var_i32();

                let unparsed_packet = UnparsedPacket::new(id, working_buf);
                let processing_result =
                    handler.handle_packet(&mut thread_ctx, connection_ctx, other_ctx, unparsed_packet, connection_ctx.inbound);

                match processing_result.0 {
                    TransformationResult::Unchanged => {
                        copy_slice_to(&read_buf.vec[pointer..next], caching_buf);
                    }
                    TransformationResult::Modified => {
                        let buffer = processing_result.1.unwrap();
                        let mut final_buffer = buffer.as_slice();

                        let mut is_uncompressed = false;
                        if compression_threshold > 0 {
                            let length = final_buffer.len();
                            if length > compression_threshold as usize {
                                compress_packet(&mut final_buffer, compressor, compression_buffer);
                            } else {
                                is_uncompressed = true;
                            }
                        }

                        // write in 2 steps to avoid extra copy
                        let len = final_buffer.len() as i32 + if is_uncompressed { 1 } else { 0 };
                        let mut frame = IndexedVec::new();
                        frame.ensure_writable(4);
                        frame.put_var_i32(len);
                        if is_uncompressed {
                            frame.put_var_i32(0);
                        }

                        copy_slice_to(frame.as_slice(), caching_buf);
                        copy_slice_to(final_buffer, caching_buf);
                    }
                    TransformationResult::Canceled => {
                        // NOOP
                    }
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
        } else {
            if connection_ctx.should_close {
                return;
            }
            break;
        }
    }
    read_buf.set_reader_index(pointer);

    buffer_read(connection_ctx, read_buf);
    write_socket(other_ctx, caching_buf);
}