use std::io;
use std::time::Duration;

#[cfg(unix)]
pub fn read_response_with_timeout<T>(
    timeout: Duration,
    parse: impl Fn(&[u8]) -> Option<T>,
) -> io::Result<Option<T>> {
    use std::io::Read;
    use std::os::unix::io::AsRawFd;
    use std::time::Instant;

    let mut stdin = io::stdin();
    let fd = stdin.as_raw_fd();
    let start = Instant::now();
    let mut buffer = Vec::new();

    loop {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Ok(None);
        }
        let remaining = timeout - elapsed;
        let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;

        let mut poll_fd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };

        let ready = unsafe { libc::poll(&mut poll_fd, 1, timeout_ms) };
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        if ready == 0 {
            return Ok(None);
        }

        let mut chunk = [0u8; 1024];
        let read = stdin.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);

        if let Some(response) = parse(&buffer) {
            return Ok(Some(response));
        }
    }
}

#[cfg(not(unix))]
pub fn read_response_with_timeout<T>(
    _timeout: Duration,
    _parse: impl Fn(&[u8]) -> Option<T>,
) -> io::Result<Option<T>> {
    Ok(None)
}
