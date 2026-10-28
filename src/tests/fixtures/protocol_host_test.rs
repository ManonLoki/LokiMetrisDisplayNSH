// 引入被测夹具
use super::*;

/// 真实的拆分写入加套接字超时,不得消耗并丢失不完整的长度前缀。
#[test]
// 真实的分段写入遇到读超时，也不能吞掉不完整的长度前缀
fn fixture_preserves_fragmented_frames_across_timeout() {
    // 本机回环上建立一对连接
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut socket, _) = listener.accept().unwrap();
    socket
        // 短读超时，模拟轮询
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let value = json!({"type":"pong","nonce":7});
    let bytes = serde_json::to_vec(&value).unwrap();
    let size = (bytes.len() as u32).to_be_bytes();
    let mut reader = FrameReader::default();
    // 先只写长度前缀的前两个字节
    peer.write_all(&size[..2]).unwrap();
    assert!(reader.poll(&mut socket).unwrap().is_none());
    assert!(reader.poll(&mut socket).unwrap().is_none()); // 两字节头部时超时
    // 补全长度前缀，再写正文的前 3 个字节
    peer.write_all(&size[2..]).unwrap();
    peer.write_all(&bytes[..3]).unwrap();
    assert!(reader.poll(&mut socket).unwrap().is_none());
    assert!(reader.poll(&mut socket).unwrap().is_none()); // 正文不完整时超时
    // 写完正文的剩余部分
    peer.write_all(&bytes[3..]).unwrap();
    assert!(reader.poll(&mut socket).unwrap().is_none());
    // 最终得到完整的帧
    assert_eq!(reader.poll(&mut socket).unwrap(), Some(value));
}

#[test]
// 只有超大的长度前缀而没有正文时，也要拒绝
fn fixture_rejects_oversized_frame_without_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut socket, _) = listener.accept().unwrap();
    peer.write_all(&u32::MAX.to_be_bytes()).unwrap();
    let mut reader = FrameReader::default();
    // 第一次读到前缀，尚不能判断
    assert!(reader.poll(&mut socket).unwrap().is_none());
    // 第二次因长度超限而报错
    assert!(reader.poll(&mut socket).is_err());
}
