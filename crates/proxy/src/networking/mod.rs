pub mod buffer_helpers;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use mio::{Events, Poll};

use packet_transformation::handling::{HandlingContext, UnparsedPacket};
use utils::contexts::{Message, NetworkThreadContext, ConnectionContext};
use utils::contexts::Message::{Threads, NewConnection};
use utils::indexed_vec::IndexedVec;
use buffer_helpers::{write_socket0, unbuffer_read, write_socket, buffer_read, copy_slice_to, validate_small_frame, read_socket};
use utils::buffers::{VarInts, VarIntsMut};
use utils::add_vec_len;
use flate2::write::{ZlibDecoder, ZlibEncoder};
use std::io::Write;
use flate2::Compression;
use packet_transformation::TransformationResult;

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
    let mut compressed_buf = IndexedVec::new();
    utils::set_vec_len(&mut compressed_buf.vec, 2048);
    let mut caching_buf = IndexedVec::new();
    utils::set_vec_len(&mut caching_buf.vec, 2048);

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
                    process_read(&mut thread_ctx, &mut player, &mut other, &mut packet_buf, &mut compressed_buf, &mut caching_buf, handler.clone());

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
                mut compression_buf: &mut IndexedVec<u8>,
                caching_buf: &mut IndexedVec<u8>,
                handler: Arc<HandlingContext>) {

    let mut pointer = 0;
    let mut next;
    read_buf.reset();
    caching_buf.reset();

    // read new packets
    unbuffer_read(connection_ctx, read_buf);
    while read_socket(connection_ctx, read_buf) {
        if connection_ctx.should_close {
            return;
        }
        if read_buf.get_writer_index() == read_buf.vec.len() {
            let len = read_buf.vec.len();
            // double size
            add_vec_len(&mut read_buf.vec, len);
            read_socket(connection_ctx, read_buf);
        } else {
            break;
        }
    }
    if connection_ctx.should_close {
        return;
    }

    let readable = read_buf.get_writer_index();

    // read all the packets
    while readable > pointer {
        if readable - pointer < 3 && !validate_small_frame(read_buf, pointer, readable) {
            break;
        }
        let mut working_buf = &read_buf.vec[pointer..];

        if let Some((packet_len, bytes)) = working_buf.get_var_i32_limit(3) {
            next = packet_len as usize + pointer + bytes as usize;

            let compression_threshold = connection_ctx.compression_threshold;
            // the full packet is available
            if readable >= next {
                if compression_threshold > 0 {
                    let real_length = working_buf.get_var_i32();
                    if real_length.0 > 0 {
                        compression_buf.reset();
                        compression_buf.ensure_writable(real_length.0 as usize);

                        {
                            //decompress
                            let mut decompressor = ZlibDecoder::new(&mut compression_buf);
                            decompressor.write_all(&working_buf[0..(packet_len - real_length.1) as usize]).unwrap();
                        }
                        working_buf = compression_buf.to_slice();
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
                        let mut final_buffer = buffer.to_slice();

                        let mut is_uncompressed = false;
                        if compression_threshold > 0 {
                            let length = final_buffer.len();
                            if length > compression_threshold as usize {
                                compression_buf.reset();
                                compression_buf.put_var_i32(length as i32);

                                {
                                    //recompress
                                    let mut compressor = ZlibEncoder::new(&mut compression_buf, Compression::fast());
                                    compressor.write_all(final_buffer).unwrap();
                                }
                                final_buffer = compression_buf.to_slice();
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

                        copy_slice_to(frame.to_slice(), caching_buf);
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
        }
    }

    buffer_read(connection_ctx, read_buf);
    write_socket(other_ctx, caching_buf);
}
