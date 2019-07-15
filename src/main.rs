use futures::try_ready;
use std::env;
use std::net::{SocketAddr, ToSocketAddrs};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

#[derive(Debug)]
pub struct Head<T> {
    stream: Option<T>,
    lines: Vec<String>,
}

impl<T> Head<T>
where
    T: Stream<Item = String>,
{
    pub fn new(stream: T) -> Self {
        Self {
            stream: Some(stream),
            lines: vec![],
        }
    }
}

impl<T> Future for Head<T>
where
    T: Stream<Item = String>,
{
    type Item = (Vec<String>, T);
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let line = match try_ready!(self.stream.as_mut().unwrap().poll()) {
                None => break,
                Some(value) => value,
            };
            if line.is_empty() {
                break;
            }
            self.lines.push(line);
        }
        Ok(Async::Ready((
            std::mem::replace(&mut self.lines, vec![]),
            self.stream.take().unwrap(),
        )))
    }
}

fn main() -> Result<(), Box<std::error::Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>()?;

    let socket = TcpListener::bind(&addr)?;
    println!("Listening on: {}", addr);

    let future = socket
        .incoming()
        .map_err(|e| println!("ERROR {:?}", e))
        .for_each(move |socket| {
            let (conn_r, conn_w) = socket.split();
            let lines = io::lines(std::io::BufReader::new(conn_r));
            let f = Head::new(lines)
                .and_then(|(head, stream)| {
                    let conn_r = stream.into_inner();
                    io::write_all(conn_w, "HTTP/1.1 200 OK\r\n\r\n").and_then(move |(conn_w, _)| {
                        let host = {
                            let mut h = head[0].split(' ');
                            h.next();
                            h.next().unwrap()
                        };
                        let addr = host.to_socket_addrs().unwrap().next().unwrap();
                        TcpStream::connect(&addr).and_then(move |socket| {
                            let (dest_r, dest_w) = socket.split();
                            tokio::spawn(
                                io::copy(conn_r, dest_w)
                                    .and_then(|_| Ok(()))
                                    .map_err(|e| println!("ERROR {:?}", e)),
                            );
                            tokio::spawn(
                                io::copy(dest_r, conn_w)
                                    .and_then(|_| Ok(()))
                                    .map_err(|e| println!("ERROR {:?}", e)),
                            );
                            Ok(())
                        })
                    })
                })
                .map_err(|e| println!("ERROR {:?}", e));
            tokio::spawn(f)
        });

    tokio::run(future);
    Ok(())
}
