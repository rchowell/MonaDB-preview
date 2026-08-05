use bytes::BytesMut;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

const PREFIX: &[u8] = b"MONA ";
const MAX_PREAMBLE_LEN: usize = 256;

#[derive(Debug, Error)]
pub enum PreambleError {
    #[error("io error reading preamble: {0}")]
    Io(#[from] std::io::Error),
    #[error("missing or malformed MONA preamble")]
    Malformed,
    #[error("preamble exceeded {MAX_PREAMBLE_LEN} bytes")]
    TooLong,
}

/// Read `MONA <db_id>\n` from the stream. Returns `(db_id, leftover bytes)`.
pub async fn read_preamble(
    stream: &mut TcpStream,
) -> Result<(String, BytesMut), PreambleError> {
    let mut buf = BytesMut::with_capacity(64);
    let mut scratch = [0u8; 64];

    loop {
        if let Some(newline) = buf.iter().position(|&b| b == b'\n') {
            let line = buf.split_to(newline + 1);
            let line = &line[..line.len() - 1]; // strip \n
            if line.ends_with(b"\r") {
                // tolerate CRLF
                let line = &line[..line.len() - 1];
                return parse_line(line, buf);
            }
            return parse_line(line, buf);
        }

        if buf.len() > MAX_PREAMBLE_LEN {
            return Err(PreambleError::TooLong);
        }

        let n = stream.read(&mut scratch).await?;
        if n == 0 {
            return Err(PreambleError::Malformed);
        }
        buf.extend_from_slice(&scratch[..n]);
    }
}

fn parse_line(line: &[u8], leftover: BytesMut) -> Result<(String, BytesMut), PreambleError> {
    if !line.starts_with(PREFIX) {
        return Err(PreambleError::Malformed);
    }
    let id = std::str::from_utf8(&line[PREFIX.len()..])
        .map_err(|_| PreambleError::Malformed)?
        .trim();
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(PreambleError::Malformed);
    }
    Ok((id.to_string(), leftover))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn reads_preamble_and_leftover() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let mut client = TcpStream::connect(addr).await.unwrap();
            client.write_all(b"MONA abc123\n\x00\x00").await.unwrap();
        });

        let (mut stream, _) = listener.accept().await.unwrap();
        let (id, leftover) = read_preamble(&mut stream).await.unwrap();
        assert_eq!(id, "abc123");
        assert_eq!(&leftover[..], b"\x00\x00");
    }
}
