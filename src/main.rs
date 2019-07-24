#![feature(await_macro, async_await)]

use std::env;
use std::net::{SocketAddr, ToSocketAddrs};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();
    let mut listener = TcpListener::bind(&addr).unwrap();
    println!("Listening on: {}", addr);

    loop {
        let (mut src, _) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            let mut buf = [0; 1024];
            let mut headers: Vec<u8> = vec![];
            while !headers.ends_with(b"\r\n\r\n") {
                let n = src.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                headers.extend(&buf[0..n]);
            }

            let mut buf = [httparse::EMPTY_HEADER; 16];
            let mut req = httparse::Request::new(&mut buf);
            req.parse(&headers).unwrap();
            if req.method != Some("CONNECT") {
                return;
            }
            src.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();

            let path = req.path.unwrap();
            let addr = path.to_socket_addrs().unwrap().next().unwrap();
            let dst = TcpStream::connect(&addr).await.unwrap();
            let (mut src_r, mut src_w) = src.split();
            let (mut dst_r, mut dst_w) = dst.split();
            tokio::spawn(async move { src_r.copy(&mut dst_w).await.map(|_| ()).unwrap() });
            tokio::spawn(async move { dst_r.copy(&mut src_w).await.map(|_| ()).unwrap() });
        });
    }
}
