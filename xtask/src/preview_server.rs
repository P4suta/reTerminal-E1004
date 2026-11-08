use std::{
    io,
    net::{Ipv4Addr, TcpListener},
};

pub fn bind(preferred_port: u16) -> io::Result<TcpListener> {
    bind_with(preferred_port, |port| {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port))
    })
}

fn bind_with<T>(preferred_port: u16, mut bind: impl FnMut(u16) -> io::Result<T>) -> io::Result<T> {
    match bind(preferred_port) {
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => bind(0),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::bind_with;
    use std::io;

    #[test]
    fn available_preferred_ports_need_one_bind_attempt() {
        let mut attempts = Vec::new();
        assert_eq!(
            bind_with(8080, |port| {
                attempts.push(port);
                Ok(port)
            })
            .unwrap(),
            8080
        );
        assert_eq!(attempts, [8080]);
    }

    #[test]
    fn only_address_conflicts_request_an_os_assigned_port() {
        for failure in [
            io::ErrorKind::AddrInUse,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::AddrNotAvailable,
            io::ErrorKind::Other,
        ] {
            let mut attempts = Vec::new();
            let result = bind_with(8080, |port| {
                attempts.push(port);
                if port == 8080 {
                    Err(io::Error::from(failure))
                } else {
                    Ok(49152)
                }
            });
            if failure == io::ErrorKind::AddrInUse {
                assert_eq!(result.unwrap(), 49152);
                assert_eq!(attempts, [8080, 0]);
            } else {
                assert_eq!(result.unwrap_err().kind(), failure);
                assert_eq!(attempts, [8080]);
            }
        }
    }

    #[test]
    fn a_failed_os_assignment_preserves_its_error_without_retrying() {
        let mut attempts = Vec::new();
        let result: io::Result<()> = bind_with(8080, |port| {
            attempts.push(port);
            Err(io::Error::from(if port == 8080 {
                io::ErrorKind::AddrInUse
            } else {
                io::ErrorKind::AddrNotAvailable
            }))
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AddrNotAvailable);
        assert_eq!(attempts, [8080, 0]);
    }
}
