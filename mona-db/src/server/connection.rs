use std::sync::Arc;

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::commands;
use crate::error::Result;
use crate::server::AppState;
use crate::wire::{take_frame, Message};

const READ_BUFFER_SIZE: usize = 8192;

pub async fn handle_connection(mut stream: TcpStream, state: Arc<AppState>) -> Result<()> {
    let mut read_buf = BytesMut::with_capacity(READ_BUFFER_SIZE);
    let mut scratch = vec![0u8; READ_BUFFER_SIZE];

    loop {
        let bytes_read = stream.read(&mut scratch).await?;
        if bytes_read == 0 {
            if read_buf.is_empty() {
                return Ok(());
            }
            return Err(crate::error::Error::Incomplete {
                needed: 4,
                available: read_buf.len(),
            });
        }

        read_buf.extend_from_slice(&scratch[..bytes_read]);

        while let Some(frame) = take_frame(&mut read_buf)? {
            let message = Message::decode(frame)?;
            let request_id = message.request_id();
            let more_to_come = message.more_to_come();
            let response_format = message.expects_reply();

            let reply = if let Some(command) = message.command_document() {
                commands::dispatch(
                    request_id,
                    &command,
                    response_format,
                    &state.registry,
                    &state.cursors,
                )
                .await?
            } else {
                None
            };

            if let Some(reply) = reply {
                if !more_to_come {
                    let encoded = reply.encode();
                    stream.write_all(&encoded).await?;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use slatedb::object_store::memory::InMemory;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use crate::cursor::CursorRegistry;
    use crate::storage::CollectionRegistry;
    use crate::wire::{MsgHeader, OpMsg, Section, OP_MSG};

    fn encode_ping(request_id: i32) -> BytesMut {
        Message::encode_msg(
            MsgHeader {
                message_length: 0,
                request_id,
                response_to: 0,
                op_code: OP_MSG,
            },
            OpMsg {
                flag_bits: 0,
                sections: vec![Section::Body(doc! { "ping": 1, "$db": "admin" })],
            },
        )
    }

    #[tokio::test]
    async fn connection_loop_replies_to_ping() {
        let state = Arc::new(AppState {
            registry: CollectionRegistry::new(Arc::new(InMemory::new()), "conn-test"),
            cursors: CursorRegistry::new(),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, state).await.unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(&encode_ping(99)).await.unwrap();

        let mut response = vec![0u8; 1024];
        let n = client.read(&mut response).await.unwrap();
        let reply = Message::decode(BytesMut::from(&response[..n])).unwrap();
        let body = reply.command_document().expect("response body");

        assert_eq!(body.get_f64("ok"), Ok(1.0));
        server.abort();
    }
}
