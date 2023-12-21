use crate::date::date::now;
use crate::datev1::datev1::now1;
mod date;
mod datev1;


use std::{
    process::{exit, Command},
    thread,
    time,
    str
};

use nix::{
    sys::wait::waitpid,
    unistd::{fork, ForkResult}
};

use std::process;
use tokio::io::{AsyncReadExt, AsyncWriteExt,Interest};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use bytes::BytesMut;
use futures::SinkExt;
use http::{header::HeaderValue, Request, Response, StatusCode};
// #[macro_use]
// extern crate serde_derive;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, Framed};


use std::io::{Write, Read};
use tokio::runtime::{Runtime, Builder};
use std::path::Path;
use std::{env, error::Error, fmt, io};
use std::thread::sleep;
use std::time::Duration;
use serde::{Serialize, Deserialize};


// fn read_header(buffer: &mut [u8]){
//     let mut p1:u16 = 0;
//     let mut p2 =p1;
//     while buffer[p2] != 0x0d && buffer[p2] != 0x0d{
//         p2 += 1 ;
//     }
// }

// #[tokio::main(flavor="current_thread")]
fn  main() {

    for i in 0..12{
        println!("{}", now1());
        sleep(Duration::from_millis(100));
    }
    unsafe {
        let children = false;
        println!("Process stared {}!",process::id());
        let sock_path = Path::new("/tmp/rust-ws.sock");


        let std_listener = std::net::TcpListener::bind("0.0.0.0:8000").unwrap();
        std_listener.set_nonblocking(true).unwrap();

        if children {
            for i in 0..8 {
                match fork().expect("Failed to fork process") {
                    ForkResult::Parent { child } => {}
                    ForkResult::Child => {
                        let child = format!("child_{}", &i.to_string());
                        println!("inside {}", child);


                        let rt = Runtime::new().unwrap();
                        rt.block_on(async {
                            let listener = TcpListener::from_std(std_listener).unwrap();
                            loop {
                                let (stream, s_addr) = listener.accept().await.unwrap();
                                rt.spawn(async move {
                                    println!("connection from {}:{} in child", s_addr.ip(),s_addr.port());

                                    if let Err(e) = process(stream).await {
                                        println!("failed to process connection; error = {}", e);
                                    }
                                });
                            }
                        });

                        exit(0);
                    }
                }
            }
        }
        let rt =Builder::new_multi_thread().enable_io().build().unwrap();
        rt.block_on(async {
            let listener = TcpListener::from_std(std_listener).unwrap();
            loop {
                let (stream, s_addr) = listener.accept().await.unwrap();
                rt.spawn(async move {
                    println!("connection from {}:{}", s_addr.ip(),s_addr.port());

                    if let Err(e) = process(stream).await {
                        println!("failed to process connection; error = {}", e);
                    }
                });
            }
        });
    }
}


async fn process(stream: TcpStream) -> Result<(), Box<dyn Error>> {
    let mut transport = Framed::new(stream, Http);

    while let Some(request) = transport.next().await {
        match request {
            Ok(request) => {
                let response = respond(request).await?;
                transport.send(response).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}


async fn respond(req: Request<()>) -> Result<Response<String>, Box<dyn Error>> {
    let mut response = Response::builder();
    let body = match req.uri().path() {
        "/plaintext" => {
            response = response.header("Content-Type", "text/plain");
            "Hello World".to_string()
        }
        "/json" => {
            response = response.header("Content-Type", "application/json");

            #[derive(Serialize)]
            struct Message {
                message: &'static str,
            }
            serde_json::to_string(&Message {
                message: "Hello World",
            })?
        }
        _ => {
            response = response.status(StatusCode::NOT_FOUND);
            String::new()
        }
    };
    let response = response
        .body(body)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

    Ok(response)
}

struct Http;

/// Implementation of encoding an HTTP response into a `BytesMut`, basically
/// just writing out an HTTP/1.1 response.
impl Encoder<Response<String>> for Http {
    type Error = io::Error;

    fn encode(&mut self, item: Response<String>, dst: &mut BytesMut) -> io::Result<()> {
        use std::fmt::Write;

        write!(
            BytesWrite(dst),
            "\
             HTTP/1.1 {}\r\n\
             Server: nginx/1.23.2\r\n\
             Content-Length: {}\r\n\
             Date: {}\r\n\
             Connection: keep-alive\r\n\
             Content-Type: application/octet-stream\r\n\
             ",
            item.status(),
            item.body().len(),
            now1()
        )
            .unwrap();

        for (k, v) in item.headers() {
            dst.extend_from_slice(k.as_str().as_bytes());
            dst.extend_from_slice(b": ");
            dst.extend_from_slice(v.as_bytes());
            dst.extend_from_slice(b"\r\n");
        }

        dst.extend_from_slice(b"\r\n");
        dst.extend_from_slice(item.body().as_bytes());

        return Ok(());

        // Right now `write!` on `Vec<u8>` goes through io::Write and is not
        // super speedy, so inline a less-crufty implementation here which
        // doesn't go through io::Error.
        struct BytesWrite<'a>(&'a mut BytesMut);

        impl fmt::Write for BytesWrite<'_> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.extend_from_slice(s.as_bytes());
                Ok(())
            }

            fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
                fmt::write(self, args)
            }
        }
    }
}

/// Implementation of decoding an HTTP request from the bytes we've read so far.
/// This leverages the `httparse` crate to do the actual parsing and then we use
/// that information to construct an instance of a `http::Request` object,
/// trying to avoid allocations where possible.
impl Decoder for Http {
    type Item = Request<()>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Request<()>>> {
        // TODO: we should grow this headers array if parsing fails and asks
        //       for more headers
        let mut headers = [None; 16];
        let (method, path, version, amt) = {
            let mut parsed_headers = [httparse::EMPTY_HEADER; 16];
            let mut r = httparse::Request::new(&mut parsed_headers);
            let status = r.parse(src).map_err(|e| {
                let msg = format!("failed to parse http request: {:?}", e);
                io::Error::new(io::ErrorKind::Other, msg)
            })?;

            let amt = match status {
                httparse::Status::Complete(amt) => amt,
                httparse::Status::Partial => return Ok(None),
            };

            let toslice = |a: &[u8]| {
                let start = a.as_ptr() as usize - src.as_ptr() as usize;
                assert!(start < src.len());
                (start, start + a.len())
            };

            for (i, header) in r.headers.iter().enumerate() {
                let k = toslice(header.name.as_bytes());
                let v = toslice(header.value);
                headers[i] = Some((k, v));
            }

            let method = http::Method::try_from(r.method.unwrap())
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            (
                method,
                toslice(r.path.unwrap().as_bytes()),
                r.version.unwrap(),
                amt,
            )
        };
        if version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "only HTTP/1.1 accepted",
            ));
        }
        let data = src.split_to(amt).freeze();
        let mut ret = Request::builder();
        ret = ret.method(method);
        let s = data.slice(path.0..path.1);
        let s = unsafe { String::from_utf8_unchecked(Vec::from(s.as_ref())) };
        ret = ret.uri(s);
        ret = ret.version(http::Version::HTTP_11);
        for header in headers.iter() {
            let (k, v) = match *header {
                Some((ref k, ref v)) => (k, v),
                None => break,
            };
            let value = HeaderValue::from_bytes(data.slice(v.0..v.1).as_ref())
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "header decode error"))?;
            ret = ret.header(&data[k.0..k.1], value);
        }

        let req = ret
            .body(())
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(Some(req))
    }
}

