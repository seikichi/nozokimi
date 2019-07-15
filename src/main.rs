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
    let listener = TcpListener::bind(&addr)?;
    println!("Listening on: {}", addr);

    let future = listener
        .incoming()
        .map_err(|e| println!("ERROR {:?}", e))
        .for_each(move |conn| {
            let f = Head::new(io::lines(std::io::BufReader::new(conn)))
                .and_then(|(head, stream)| {
                    let conn = stream.into_inner().into_inner();
                    (io::write_all(conn, "HTTP/1.1 200 OK\r\n\r\n"), Ok(head))
                })
                .and_then(move |((conn, _), head)| {
                    let addr = {
                        let h = head[0].split(' ').collect::<Vec<_>>();
                        h[1].to_socket_addrs().unwrap().next().unwrap()
                    };
                    TcpStream::connect(&addr).and_then(move |dest| {
                        let (conn_r, conn_w) = conn.split();
                        let (dest_r, dest_w) = dest.split();
                        tokio::spawn(io::copy(conn_r, dest_w).map(|_| ()).map_err(|_| ()));
                        tokio::spawn(io::copy(dest_r, conn_w).map(|_| ()).map_err(|_| ()));
                        Ok(())
                    })
                })
                .map_err(|e| println!("ERROR {:?}", e));
            tokio::spawn(f)
        });

    tokio::run(future);
    Ok(())
}
