use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

const DEADLINE: Duration = Duration::from_secs(10);

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            self.0.kill().unwrap();
        }
        self.0.wait().unwrap();
    }
}

fn start(port: u16) -> (Server, SocketAddr) {
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["serve", &port.to_string()])
            .current_dir(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap(),
            )
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = server.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = sender.send(result);
    });
    let line = receiver.recv_timeout(DEADLINE).unwrap().unwrap();
    let address: SocketAddr = line
        .trim()
        .strip_prefix("Preview: http://")
        .unwrap_or_else(|| panic!("server did not announce a listening address: {line:?}"))
        .parse()
        .unwrap();
    assert_eq!(address.ip(), Ipv4Addr::LOCALHOST);
    assert_ne!(address.port(), 0);
    (server, address)
}

fn request(address: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect_timeout(&address, DEADLINE).unwrap();
    stream.set_read_timeout(Some(DEADLINE)).unwrap();
    stream.set_write_timeout(Some(DEADLINE)).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[test]
fn occupied_ports_keep_their_owner_and_preview_uses_its_announced_address() {
    let owner = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let occupied = owner.local_addr().unwrap();
    let (_server, address) = start(occupied.port());
    assert_ne!(address, occupied);
    assert!(request(address, "/").contains("<title>reTerminal E1004</title>"));
    assert!(request(address, "/?tools").contains("id=\"tab-tools\""));
    assert_eq!(owner.local_addr().unwrap(), occupied);
    assert_eq!(
        TcpListener::bind(occupied).unwrap_err().kind(),
        std::io::ErrorKind::AddrInUse
    );
}

#[test]
fn simultaneous_previews_keep_distinct_nonzero_ports_until_their_owner_exits() {
    let (first, first_address) = start(0);
    let (_second, second_address) = start(0);
    assert_ne!(first_address, second_address);
    assert!(request(first_address, "/style.css").contains("text/css"));
    assert!(request(second_address, "/private.txt").starts_with("HTTP/1.1 404"));
    drop(first);
    assert!(TcpStream::connect_timeout(&first_address, DEADLINE).is_err());
    assert!(request(second_address, "/").starts_with("HTTP/1.1 200"));
}
