use std::io::{self, Write};
use std::net::TcpStream;


fn main() {
    let mut stream = TcpStream::connect("127.0.0.1:7878").unwrap();
    
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read line");
        stream.write_all(input.as_bytes()).expect("Failed to send message");
    }
}
