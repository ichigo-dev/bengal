/*

    Sender

*/

use crate::{ Channel, Strategy, Blocking, NonBlocking };
use crate::error::{ SendError, TrySendError };

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::process;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::task::{ Poll, Context };

use concurrent_queue::PushError;
use event_listener::EventListener;

pub struct Sender<T>
{
    pub(crate) channel: Arc<Channel<T>>,
}

impl<T> Sender<T>
{
    //--------------------------------------------------------------------------
    //  Attempts to send a message to the channel.
    //
    //  Returns an error if the queue is full or the channel is closed.
    //--------------------------------------------------------------------------
    pub fn try_send( &self, msg: T ) -> Result<(), TrySendError<T>>
    {
        match self.channel.queue.push(msg)
        {
            Ok(()) =>
            {
                self.channel.recv_event.notify_additional(1);
                self.channel.stream_event.notify(usize::MAX);
                Ok(())
            },
            Err(PushError::Full(msg)) => Err(TrySendError::Full(msg)),
            Err(PushError::Closed(msg)) => Err(TrySendError::Closed(msg)),
        }
    }

    //--------------------------------------------------------------------------
    //  Sends a message to the channel.
    //
    //  If the queue is full, wait until there is room for the message.
    //
    //  Returns an error if the queue is closed.
    //--------------------------------------------------------------------------
    pub fn send( &self, msg: T ) -> Send<'_, T>
    {
        Send
        {
            sender: self,
            listener: None,
            msg: Some(msg),
        }
    }

    //--------------------------------------------------------------------------
    //  Sends a message to the channel with blocking. Note that using this 
    //  method blocks the thread until the message is sent.
    //
    //  If the queue is full, block until there is room for the message.
    //
    //  Returns an error if the queue is closed.
    //--------------------------------------------------------------------------
    pub fn send_blocking( &self, msg: T ) -> Result<(), SendError<T>>
    {
        self.send(msg).wait()
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

impl<T> fmt::Debug for Sender<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when debugging.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        write!(f, "Sender {{ .. }}")
    }
}

impl<T> Drop for Sender<T>
{
    //--------------------------------------------------------------------------
    //  Decrement the sender count and close the channel if it drops down to 
    //  zero.
    //--------------------------------------------------------------------------
    fn drop( &mut self )
    {
        if self.channel.sender_count.fetch_sub(1, Ordering::AcqRel) == 1
        {
            self.channel.close();
        }
    }
}

impl<T> Clone for Sender<T>
{
    //--------------------------------------------------------------------------
    //  Clones the sender.
    //--------------------------------------------------------------------------
    fn clone( &self ) -> Sender<T>
    {
        let count = self.channel.sender_count.fetch_add(1, Ordering::Relaxed);

        if count > (usize::MAX / 2)
        {
            process::abort();
        }

        Sender
        {
            channel: self.channel.clone(),
        }
    }
}

//------------------------------------------------------------------------------
//  A future in which the sender sends a message.
//------------------------------------------------------------------------------
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Send<'a, T>
{
    sender: &'a Sender<T>,
    listener: Option<EventListener>,
    msg: Option<T>,
}

impl<'a, T> Send<'a, T>
{
    //--------------------------------------------------------------------------
    //  Runs this future with the given strategy.
    //--------------------------------------------------------------------------
    pub fn run<S: Strategy>( &mut self, cx: &mut S::Context )
        -> Poll<Result<(), SendError<T>>>
    {
        loop
        {
            let msg = self.msg.take().unwrap();

            //  Attempt to send a message.
            match self.sender.try_send(msg)
            {
                Ok(()) => return Poll::Ready(Ok(())),
                Err(TrySendError::Closed(msg)) =>
                {
                    return Poll::Ready(Err(SendError(msg)));
                },
                Err(TrySendError::Full(m)) => self.msg = Some(m),
            }

            //  Listen for event notifications if the queue is full.
            match self.listener.take()
            {
                None =>
                {
                    //  Start listening and try sending the message again.
                    self.listener = Some(
                        self.sender.channel.send_event.listen()
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
    fn wait( mut self ) -> Result<(), SendError<T>>
    {
        match self.run::<Blocking>(&mut ())
        {
            Poll::Ready(res) => res,
            Poll::Pending => unreachable!(),
        }
    }
}

impl<'a, T> Unpin for Send<'a, T> {}

impl<'a, T> Future for Send<'a, T>
{
    type Output = Result<(), SendError<T>>;

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
