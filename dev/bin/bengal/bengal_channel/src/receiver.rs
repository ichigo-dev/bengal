/*

    Receiver

*/

use crate::{ Channel, Strategy, Blocking, NonBlocking };
use crate::error::{ TryRecvError, RecvError };

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::process;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::task::{ Poll, Context };

use concurrent_queue::PopError;
use event_listener::EventListener;
use futures_core::stream::{ Stream, FusedStream };

pub struct Receiver<T>
{
    pub(crate) channel: Arc<Channel<T>>,
    pub(crate) listener: Option<EventListener>,
}

impl<T> Receiver<T>
{
    //--------------------------------------------------------------------------
    //  Attempts to receive a message from the channel.
    //
    //  Returns an error if the queue is empty.
    //--------------------------------------------------------------------------
    pub fn try_recv( &self ) -> Result<T, TryRecvError>
    {
        match self.channel.queue.pop()
        {
            Ok(msg) =>
            {
                self.channel.send_event.notify_additional(1);
                Ok(msg)
            },
            Err(PopError::Empty) => Err(TryRecvError::Empty),
            Err(PopError::Closed) => Err(TryRecvError::Closed),
        }
    }

    //--------------------------------------------------------------------------
    //  Receives a message from the channel.
    //
    //  If the queue is empty, wait for new messages to be sent.
    //--------------------------------------------------------------------------
    pub fn recv( &self ) -> Recv<'_, T>
    {
        Recv
        {
            receiver: self,
            listener: None,
        }
    }

    //--------------------------------------------------------------------------
    //  Receives a message from the chnanel with blocking. Note that using this 
    //  method blocks the thread until the message is received.
    //
    //  If the queue is empty, wait for new messages to be sent.
    //--------------------------------------------------------------------------
    pub fn recv_blocking( &self ) -> Result<T, RecvError>
    {
        self.recv().wait()
    }

    //--------------------------------------------------------------------------
    //  Closes channel manually.
    //--------------------------------------------------------------------------
    pub fn close( &self ) -> bool
    {
        self.channel.close()
    }

    //--------------------------------------------------------------------------
    //  Checks if message queue is empty.
    //--------------------------------------------------------------------------
    pub fn is_empty( &self ) -> bool
    {
        self.channel.queue.is_empty()
    }

    //--------------------------------------------------------------------------
    //  Checks if message queue is full.
    //--------------------------------------------------------------------------
    pub fn is_full( &self ) -> bool
    {
        self.channel.queue.is_full()
    }

    //--------------------------------------------------------------------------
    //  Returns the number of messages present in the channel.
    //--------------------------------------------------------------------------
    pub fn len( &self ) -> usize
    {
        self.channel.queue.len()
    }

    //--------------------------------------------------------------------------
    //  Returns the capacity of the channel.
    //--------------------------------------------------------------------------
    pub fn capacity( &self ) -> Option<usize>
    {
        self.channel.queue.capacity()
    }

    //--------------------------------------------------------------------------
    //  Returns the number of senders of the channel.
    //--------------------------------------------------------------------------
    pub fn sender_count( &self ) -> usize
    {
        self.channel.sender_count.load(Ordering::SeqCst)
    }

    //--------------------------------------------------------------------------
    //  Returns the number of receivers of the channel.
    //--------------------------------------------------------------------------
    pub fn receiver_count( &self ) -> usize
    {
        self.channel.receiver_count.load(Ordering::SeqCst)
    }
}

impl<T> fmt::Debug for Receiver<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when debugging.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        write!(f, "Receiver {{ .. }}")
    }
}

impl<T> Drop for Receiver<T>
{
    //--------------------------------------------------------------------------
    //  Decrement the receiver count and close the channel if it drops down to 
    //  zero.
    //--------------------------------------------------------------------------
    fn drop( &mut self )
    {
        if self.channel.receiver_count.fetch_sub(1, Ordering::AcqRel) == 1
        {
            self.channel.close();
        }
    }
}

impl<T> Clone for Receiver<T>
{
    //--------------------------------------------------------------------------
    //  Clones the receiver.
    //--------------------------------------------------------------------------
    fn clone( &self ) -> Receiver<T>
    {
        let count = self.channel.receiver_count.fetch_add(1, Ordering::Relaxed);

        if count > (usize::MAX / 2)
        {
            process::abort();
        }

        Receiver
        {
            channel: self.channel.clone(),
            listener: None,
        }
    }
}

impl<T> Stream for Receiver<T>
{
    type Item = T;

    //--------------------------------------------------------------------------
    //  Attempts to pull out the next value of this stream, registering the 
    //  current task for wakeup if the value is not yet available, and 
    //  returning `None` if the stream is exhausted.
    //--------------------------------------------------------------------------
    fn poll_next( mut self: Pin<&mut Self>, cx: &mut Context<'_> )
        -> Poll<Option<Self::Item>>
    {
        loop
        {
            if let Some(listener) = self.listener.as_mut()
            {
                futures_core::ready!(Pin::new(listener).poll(cx));
                self.listener = None;
            }

            loop
            {
                //  Attempt to receive a message.
                match self.try_recv()
                {
                    Ok(msg) =>
                    {
                        self.listener = None;
                        return Poll::Ready(Some(msg));
                    },
                    Err(TryRecvError::Closed) =>
                    {
                        self.listener = None;
                        return Poll::Ready(None);
                    },
                    Err(TryRecvError::Empty) => {},
                }

                //  Listen for event notifications if the queue is empty.
                match self.listener.take()
                {
                    None =>
                    {
                        //  Start listening and try receiving the message again.
                        self.listener = Some(
                            self.channel.stream_event.listen()
                        );
                    },
                    Some(_) =>
                    {
                        //  Go back to the outer loop to poll the listener.
                        break;
                    },
                }
            }
        }
    }
}

impl<T> FusedStream for Receiver<T>
{
    //--------------------------------------------------------------------------
    //  Return `true` if the stream should no longer be polled.
    //--------------------------------------------------------------------------
    fn is_terminated( &self ) -> bool
    {
        self.channel.queue.is_closed() && self.channel.queue.is_empty()
    }
}

//------------------------------------------------------------------------------
//  A future in which the receiver receives a message.
//------------------------------------------------------------------------------
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Recv<'a, T>
{
    receiver: &'a Receiver<T>,
    listener: Option<EventListener>,
}

impl<'a, T> Recv<'a, T>
{
    //--------------------------------------------------------------------------
    //  Runs this future with the given strategy.
    //--------------------------------------------------------------------------
    fn run<S: Strategy>( &mut self, cx: &mut S::Context )
        -> Poll<Result<T, RecvError>>
    {
        loop
        {
            //  Attempt to receive a message.
            match self.receiver.try_recv()
            {
                Ok(msg) => return Poll::Ready(Ok(msg)),
                Err(TryRecvError::Closed) => return Poll::Ready(Err(RecvError)),
                Err(TryRecvError::Empty) => {},
            }

            //  Listen for event notifications if the message queue is empty.
            match self.listener.take()
            {
                None =>
                {
                    //  Start listening and try receiving the message again.
                    self.listener = Some(
                        self.receiver.channel.recv_event.listen()
                    );
                },
                Some(listener) =>
                {
                    if let Err(listener) = S::poll(listener, cx)
                    {
                        self.listener = Some(listener);
                        return Poll::Pending;
                    }
                },
            }
        }
    }

    //--------------------------------------------------------------------------
    //  Runs this future with blocking strategy.
    //--------------------------------------------------------------------------
    fn wait( mut self ) -> Result<T, RecvError>
    {
        match self.run::<Blocking>(&mut ())
        {
            Poll::Ready(res) => res,
            Poll::Pending => unreachable!(),
        }
    }
}

impl<'a, T> Unpin for Recv<'a, T> {}

impl<'a, T> Future for Recv<'a, T>
{
    type Output = Result<T, RecvError>;

    //--------------------------------------------------------------------------
    //  Attempts to resolve the future to a final value, registering the 
    //  current task for wakeup if the value is not yet available.
    //--------------------------------------------------------------------------
    fn poll( mut self: Pin<&mut Self>, cx: &mut Context<'_> )
        -> Poll<Self::Output>
    {
        self.run::<NonBlocking<'_>>(cx)
    }
}
