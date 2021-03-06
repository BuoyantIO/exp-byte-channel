extern crate byte_channel;
extern crate bytes;
extern crate futures;
extern crate test_futures;

use bytes::*;
use byte_channel::*;
use futures::{Async, Poll, Stream, executor};
use test_futures::*;

struct Reader(sync::ByteReceiver<()>, usize);
impl Reader {
    fn resize(self, sz: usize) -> Reader {
        Reader(self.0, sz)
    }

    fn shrink_window(&mut self, decr: usize) {
        self.0.shrink_window(decr);
    }
}

impl Stream for Reader {
    type Item = sync::Chunk;
    type Error = ();
    fn poll(&mut self) -> Poll<Option<sync::Chunk>, ()> {
        self.0.poll_chunk(self.1)
    }
}

#[test]
fn consuming_chunks_frees_space() {
    let (mut wx, mut tx, rx) = sync::new::<()>(10);
    let mut rx = Reader(rx, 0);

    assert_eq!(tx.available_window(), 0);
    sassert_next(&mut wx, 10);
    assert_eq!(tx.available_window(), 10);

    tx.push_bytes(Bytes::from("0123456789")).unwrap();
    sassert_empty(&mut wx);

    for sz in &[4 as usize, 3, 2, 1] {
        // Read a chunk from the receiver.
        let sz = *sz;
        rx = rx.resize(sz);
        let chunk = match executor::spawn(&mut rx).poll_stream_notify(&notify_panic(), 0) {
            Ok(Async::Ready(Some(c))) => c,
            res => panic!("stream error: {:?}", res),
        };
        sassert_empty(&mut wx);
        assert_eq!(chunk.remaining(), sz);

        // Drop the chunk and ensure that the space has been advertised as freed.
        drop(chunk);
        assert_eq!(tx.available_window(), 0);
        sassert_next(&mut wx, sz);
        assert_eq!(tx.available_window(), sz);

        // Backfill freed data immediately.
        tx.push_bytes(Bytes::from(vec![0; sz])).unwrap();
        assert_eq!(tx.available_window(), 0);
    }
    sassert_empty(&mut wx);

    rx = rx.resize(8);
    let mut chunk = match executor::spawn(&mut rx).poll_stream_notify(&notify_panic(), 0) {
        Ok(Async::Ready(Some(c))) => c,
        res => panic!("stream error: {:?}", res),
    };
    assert_eq!(chunk.remaining(), 8);
    sassert_empty(&mut wx);

    //
    // Validate that partially consuming a chunk frees space on the channel.
    //

    chunk.advance(4);
    assert_eq!(chunk.remaining(), 4);
    sassert_next(&mut wx, 4);
    assert_eq!(tx.available_window(), 4);

    chunk.advance(3);
    assert_eq!(chunk.remaining(), 1);
    sassert_next(&mut wx, 3);
    assert_eq!(tx.available_window(), 7);

    drop(chunk);
    sassert_next(&mut wx, 1);
    assert_eq!(tx.available_window(), 8);
    sassert_empty(&mut wx);

    let chunk = match executor::spawn(&mut rx).poll_stream_notify(&notify_panic(), 0) {
        Ok(Async::Ready(Some(c))) => c,
        res => panic!("stream error: {:?}", res),
    };
    sassert_empty(&mut wx);
    assert_eq!(chunk.remaining(), 2);
    drop(chunk);
    assert_eq!(tx.available_window(), 8);
    sassert_next(&mut wx, 2);
    assert_eq!(tx.available_window(), 10);
    sassert_empty(&mut wx);

    drop(tx);
    sassert_done(&mut rx);

    drop(rx);
    sassert_done(&mut wx);
}


#[test]
fn shrink_between_advertise_and_push() {
    let (mut wx, mut tx, rx) = sync::new::<()>(10);
    let mut rx = Reader(rx, 10);
    assert_eq!(tx.available_window(), 0);

    sassert_next(&mut wx, 10);
    assert_eq!(tx.available_window(), 10);

    rx.shrink_window(10);
    assert_eq!(tx.available_window(), 10);

    tx.push_bytes(Bytes::from("0123456789")).unwrap();
    sassert_empty(&mut wx);
    let chunk = match executor::spawn(&mut rx).poll_stream_notify(&notify_panic(), 0) {
        Ok(Async::Ready(Some(c))) => c,
        res => panic!("stream error: {:?}", res),
    };
    sassert_empty(&mut wx);
    drop(chunk);
    assert_eq!(tx.available_window(), 0);

}
