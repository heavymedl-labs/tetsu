//! Runtime suite, Linux only: the same minimal HTTP/1.1 keep-alive responder
//! on Tokio and on io_uring runtimes. `run.sh` pins it to one core.

const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nContent-Type: text/plain\r\n\r\nHello, World!";

/// Counts complete requests in `buf` and drops them, keeping any partial tail.
fn take_requests(buf: &mut Vec<u8>) -> usize {
    let (mut n, mut start) = (0, 0);
    while let Some(p) = buf[start..].windows(4).position(|w| w == b"\r\n\r\n") {
        start += p + 4;
        n += 1;
    }
    buf.drain(..start);
    n
}

fn responses(buf: &mut Vec<u8>) -> Vec<u8> {
    RESPONSE.repeat(take_requests(buf))
}

async fn tokio_listen(addr: String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    loop {
        let (mut s, _) = listener.accept().await.unwrap();
        let _ = s.set_nodelay(true);
        tokio::spawn(async move {
            let (mut buf, mut chunk) = (Vec::with_capacity(4096), [0u8; 4096]);
            loop {
                let n = match s.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&chunk[..n]);
                if s.write_all(&responses(&mut buf)).await.is_err() {
                    return;
                }
            }
        });
    }
}

pub fn serve(runtime: &str, port: u16) {
    let addr = format!("[::]:{port}");
    match runtime {
        "tokio-ct" => tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(tokio_listen(addr)),
        "tokio-mt" => tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
            .block_on(tokio_listen(addr)),
        "monoio" => {
            let mut rt = monoio::RuntimeBuilder::<monoio::IoUringDriver>::new().enable_timer().build().unwrap();
            rt.block_on(async move {
                use monoio::io::{AsyncReadRent, AsyncWriteRentExt};
                let listener = monoio::net::TcpListener::bind(addr).unwrap();
                loop {
                    let (mut s, _) = listener.accept().await.unwrap();
                    let _ = s.set_nodelay(true);
                    monoio::spawn(async move {
                        let (mut buf, mut chunk) = (Vec::with_capacity(4096), vec![0u8; 4096]);
                        loop {
                            let (r, c) = s.read(chunk).await;
                            chunk = c;
                            let n = match r {
                                Ok(0) | Err(_) => return,
                                Ok(n) => n,
                            };
                            buf.extend_from_slice(&chunk[..n]);
                            if s.write_all(responses(&mut buf)).await.0.is_err() {
                                return;
                            }
                        }
                    });
                }
            });
        }
        "compio" => compio::runtime::Runtime::new().unwrap().block_on(async move {
            use compio::io::{AsyncRead, AsyncWriteExt};
            let listener = compio::net::TcpListener::bind(addr).await.unwrap();
            loop {
                let (mut s, _) = listener.accept().await.unwrap();
                let _ = s.set_nodelay(true);
                compio::runtime::spawn(async move {
                    let (mut buf, mut chunk) = (Vec::with_capacity(4096), Vec::with_capacity(4096));
                    loop {
                        chunk.clear();
                        let compio::BufResult(r, c) = s.read(chunk).await;
                        chunk = c;
                        let n = match r {
                            Ok(0) | Err(_) => return,
                            Ok(n) => n,
                        };
                        buf.extend_from_slice(&chunk[..n]);
                        if s.write_all(responses(&mut buf)).await.0.is_err() {
                            return;
                        }
                    }
                })
                .detach();
            }
        }),
        other => panic!("unknown runtime {other}"),
    }
}
