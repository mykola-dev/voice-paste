//! Synchronous WinHTTP is confined to the owned provisioning thread. OS TLS and
//! automatic HTTPS redirects; no interpreter, browser or external downloader.
use std::{
    ffi::c_void,
    io::{self, Read},
    ptr::{null, null_mut},
};
use windows_sys::Win32::Networking::WinHttp::*;
struct Handle(*mut c_void);
impl Handle {
    fn new(pointer: *mut c_void) -> Result<Self, String> {
        if pointer.is_null() {
            Err(io::Error::last_os_error().to_string())
        } else {
            Ok(Self(pointer))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        /* SAFETY: unique WinHTTP handle. */
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
pub struct Response {
    request: Handle,
    _connection: Handle,
    _session: Handle,
    pub status: u32,
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn checked(result: i32) -> Result<(), String> {
    if result == 0 {
        Err(io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

impl Response {
    pub fn get(
        host: &str,
        port: u16,
        path: &str,
        secure: bool,
        offset: u64,
    ) -> Result<Self, String> {
        // SAFETY: live NUL-terminated strings; handles are owned, synchronous and
        // destroyed request → connection → session. No TLS checks are disabled.
        unsafe {
            let session = Handle::new(WinHttpOpen(
                wide("VoicePaste/0.1").as_ptr(),
                if secure {
                    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
                } else {
                    WINHTTP_ACCESS_TYPE_NO_PROXY
                },
                null(),
                null(),
                0,
            ))?;
            checked(WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 5000))?;
            let connection = Handle::new(WinHttpConnect(session.0, wide(host).as_ptr(), port, 0))?;
            let request = Handle::new(WinHttpOpenRequest(
                connection.0,
                wide("GET").as_ptr(),
                wide(path).as_ptr(),
                null(),
                null(),
                null(),
                if secure { WINHTTP_FLAG_SECURE } else { 0 },
            ))?;
            let headers = wide(&format!(
                "Accept-Encoding: identity\r\nRange: bytes={offset}-\r\n"
            ));
            checked(WinHttpSendRequest(
                request.0,
                headers.as_ptr(),
                (headers.len() - 1) as u32,
                null(),
                0,
                0,
                0,
            ))?;
            checked(WinHttpReceiveResponse(request.0, null_mut()))?;
            let mut status = 0u32;
            let mut bytes = 4;
            checked(WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&mut status as *mut u32).cast(),
                &mut bytes,
                null_mut(),
            ))?;
            Ok(Self {
                request,
                _connection: connection,
                _session: session,
                status,
            })
        }
    }
    pub fn validate_range(&self, offset: u64, total: u64) -> Result<(), String> {
        let mut value = [0u16; 256];
        let mut bytes = (value.len() * 2) as u32;
        // SAFETY: bounded writable UTF-16 output and live request handle.
        unsafe {
            checked(WinHttpQueryHeaders(
                self.request.0,
                WINHTTP_QUERY_CONTENT_RANGE,
                null(),
                value.as_mut_ptr().cast(),
                &mut bytes,
                null_mut(),
            ))?;
        }
        let value = String::from_utf16_lossy(
            &value[..value.iter().position(|&v| v == 0).unwrap_or(value.len())],
        );
        super::validate_range(&value, offset, total)
    }
}
impl Read for Response {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut count = 0;
        // SAFETY: OS writes at most the bounded buffer size and sets count.
        if unsafe {
            WinHttpReadData(
                self.request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len().min(u32::MAX as usize) as u32,
                &mut count,
            )
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(count as usize)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };
    #[test]
    fn native_http_range_transport_uses_local_fixture_only() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let task = thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            assert!(headers.to_ascii_lowercase().contains("range: bytes=3-"));
            socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 3-5/6\r\nConnection: close\r\n\r\ndef").unwrap();
        });
        let mut response = Response::get("127.0.0.1", port, "/fixture", false, 3).unwrap();
        assert_eq!(response.status, 206);
        response.validate_range(3, 6).unwrap();
        let mut bytes = Vec::new();
        response.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"def");
        task.join().unwrap();
    }
}
