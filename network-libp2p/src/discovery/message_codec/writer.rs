use std::{
    io::Write,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Buf, BufMut, BytesMut};
use futures::{ready, AsyncWrite, Sink};
use pin_project::pin_project;
use serde::Serialize;

use super::header::Header;

fn write_from_buf<W>(
    inner: &mut W,
    buffer: &mut BytesMut,
    cx: &mut Context,
) -> Poll<Result<(), postcard::Error>>
where
    W: AsyncWrite + Unpin,
{
    if buffer.remaining() > 0 {
        match Pin::new(inner).poll_write(cx, buffer.chunk()) {
            Poll::Ready(Ok(0)) => {
                warn!("MessageWriter: write_from_buf: Unexpected EOF.");
                Poll::Ready(Err(postcard::Error::SerdeSerCustom))
            }

            Poll::Ready(Ok(n)) => {
                buffer.advance(n);
                if buffer.remaining() > 0 {
                    Poll::Pending
                } else {
                    buffer.clear();
                    Poll::Ready(Ok(()))
                }
            }

            Poll::Ready(Err(_)) => Poll::Ready(Err(postcard::Error::SerdeSerCustom)),

            Poll::Pending => Poll::Pending,
        }
    } else {
        Poll::Ready(Ok(()))
    }
}

#[pin_project]
pub struct MessageWriter<W, M> {
    inner: W,
    buffer: BytesMut,
    _message_type: PhantomData<M>,
}

impl<W, M> MessageWriter<W, M> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            buffer: BytesMut::new(),
            _message_type: PhantomData,
        }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W, M> Sink<&M> for MessageWriter<W, M>
where
    W: AsyncWrite + Unpin,
    M: Serialize + std::fmt::Debug,
{
    type Error = postcard::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let self_projected = self.project();

        // Try to write from buffer to the inner `AsyncWrite`
        match write_from_buf(self_projected.inner, self_projected.buffer, cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
        }
    }

    fn start_send(self: Pin<&mut Self>, item: &M) -> Result<(), Self::Error> {
        let self_projected = self.project();

        if !self_projected.buffer.is_empty() {
            warn!("MessageWriter: Trying to send while buffer is not empty");
            return Err(postcard::Error::SerdeSerCustom);
        }

        // Reserve space for the header and message.
        let ser_item = postcard::to_allocvec(&item)?;
        self_projected.buffer.reserve(ser_item.len() + Header::SIZE);

        let header = Header::new(ser_item.len() as u32);
        let mut ser_header = [0u8; Header::SIZE];
        postcard::to_slice(&header, &mut ser_header)?;

        let mut w = self_projected.buffer.writer();

        // Write header
        w.write_all(&ser_header)
            .map_err(|_| postcard::Error::SerdeSerCustom)?;

        // Serialize the message into the buffer.
        w.write_all(&ser_item)
            .map_err(|_| postcard::Error::SerdeSerCustom)?;

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let self_projected = self.project();

        // Try to finish writing from buffer to the inner `AsyncWrite`
        match write_from_buf(self_projected.inner, self_projected.buffer, cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Ready(Ok(())) => {
                // Finished writing the message. Flush the underlying `AsyncWrite`.
                Poll::Ready(
                    ready!(Pin::new(self_projected.inner).poll_flush(cx))
                        .map_err(|_| postcard::Error::SerdeSerCustom),
                )
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let self_projected = self.project();

        // Try to finish writing from buffer to the inner `AsyncWrite`
        match write_from_buf(self_projected.inner, self_projected.buffer, cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Ready(Ok(())) => {
                // Finished writing the message. Close the underlying `AsyncWrite`.
                Poll::Ready(
                    ready!(Pin::new(self_projected.inner).poll_close(cx))
                        .map_err(|_| postcard::Error::SerdeSerCustom),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::SinkExt;
    use nimiq_test_log::test;
    use serde::{Deserialize, Serialize};

    use super::{Header, MessageWriter};

    #[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
    struct TestMessage {
        pub foo: u32,
        pub bar: String,
    }

    #[test(tokio::test)]
    pub async fn it_can_write_a_message() {
        let test_message = TestMessage {
            foo: 42,
            bar: "Hello World".to_owned(),
        };

        let mut message_writer = MessageWriter::new(vec![]);

        message_writer.send(&test_message).await.unwrap();

        let data = message_writer.into_inner();

        assert_eq!(
            postcard::to_allocvec(&test_message).unwrap(),
            &data[Header::SIZE..]
        )
    }
}
