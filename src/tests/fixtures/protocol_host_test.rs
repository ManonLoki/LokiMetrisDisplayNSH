use super::*;

/// Real split writes with socket timeouts must not consume and lose partial length prefixes.
#[test]
fn fixture_preserves_fragmented_frames_across_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut socket, _) = listener.accept().unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let value = json!({"type":"pong","nonce":7});
    let bytes = serde_json::to_vec(&value).unwrap();
    let size = (bytes.len() as u32).to_be_bytes();
    let mut reader = FrameReader::default();
    peer.write_all(&size[..2]).unwrap();
    assert!(reader.poll(&mut socket).unwrap().is_none());
    assert!(reader.poll(&mut socket).unwrap().is_none()); // timeout with two-byte header
    peer.write_all(&size[2..]).unwrap();
    peer.write_all(&bytes[..3]).unwrap();
    assert!(reader.poll(&mut socket).unwrap().is_none());
    assert!(reader.poll(&mut socket).unwrap().is_none()); // timeout with incomplete body
    peer.write_all(&bytes[3..]).unwrap();
    assert!(reader.poll(&mut socket).unwrap().is_none());
    assert_eq!(reader.poll(&mut socket).unwrap(), Some(value));
}

#[test]
fn fixture_rejects_oversized_frame_without_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut socket, _) = listener.accept().unwrap();
    peer.write_all(&u32::MAX.to_be_bytes()).unwrap();
    let mut reader = FrameReader::default();
    assert!(reader.poll(&mut socket).unwrap().is_none());
    assert!(reader.poll(&mut socket).is_err());
}
