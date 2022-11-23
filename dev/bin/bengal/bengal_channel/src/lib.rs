/*

    Async channel

    ----------------------------------------------------------------------------

    An asynchronous channel that passes messages between threads. A channel can 
    have multiple senders and receivers.

    Bounded queues with limited capacity and Unbounded queues with unlimited 
    capacity can be used as channels.

    A channel is closed when all senders and receivers have been dropped. When 
    a channel is closed no further messages can be sent, but reception is 
    possible. A channel can also be closed manually.

*/

mod error;
mod sender;
mod receiver;

use sender::Sender;
use receiver::Receiver;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::task::{ Context, Poll };

use concurrent_queue::ConcurrentQueue;
use event_listener::{ Event, EventListener };

//------------------------------------------------------------------------------
//  A channel through which messages are thrown from senders to receivers.
//------------------------------------------------------------------------------
struct Channel<T>
{
    //  Inner message queue.
    queue: ConcurrentQueue<T>,

    //  Event notified when a message is received while the queue is full.
    send_event: Event,

    //  Event notified when a message is sent while the queue is empty.
    recv_event: Event,

    //  Events used for stream control during message transmission.
    stream_event: Event,

    //  Number of active senders.
    sender_count: AtomicUsize,

    //  Number of active receivers.
    receiver_count: AtomicUsize,
}

impl<T> Channel<T>
{
    //--------------------------------------------------------------------------
    //  Closes channel manually.
    //--------------------------------------------------------------------------
    pub fn close( &self ) -> bool
    {
        if self.queue.close() == true
        {
            //  Notify event to all listeners.
            self.send_event.notify(usize::MAX);
            self.recv_event.notify(usize::MAX);
            self.stream_event.notify(usize::MAX);

            true
        }
        else
        {
            false
        }
    }
}

//------------------------------------------------------------------------------
//  Creates a sender-receiver pair that uses a limited capacity channel.
//------------------------------------------------------------------------------
pub fn bounded<T>( cap: usize ) -> (Sender<T>, Receiver<T>)
{
    assert!(cap > 0, "capacity cannot be zero");

    let channel = Arc::new(Channel
    {
        queue: ConcurrentQueue::bounded(cap),
        send_event: Event::new(),
        recv_event: Event::new(),
        stream_event: Event::new(),
        sender_count: AtomicUsize::new(1),
        receiver_count: AtomicUsize::new(1),
    });

    let s = Sender
    {
        channel: channel.clone(),
    };

    let r = Receiver
    {
        channel,
        listener: None,
    };

    (s, r)
}

//------------------------------------------------------------------------------
//  Creates a sender-receiver pair that uses a unlimited capacity channel.
//------------------------------------------------------------------------------
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>)
{
    let channel = Arc::new(Channel
    {
        queue: ConcurrentQueue::unbounded(),
        send_event: Event::new(),
        recv_event: Event::new(),
        stream_event: Event::new(),
        sender_count: AtomicUsize::new(1),
        receiver_count: AtomicUsize::new(1),
    });

    let s = Sender
    {
        channel: channel.clone(),
    };

    let r = Receiver
    {
        channel,
        listener: None,
    };

    (s, r)
}

//------------------------------------------------------------------------------
//  A strategy used to poll an `EventListener`.
//------------------------------------------------------------------------------
pub trait Strategy
{
    type Context;

    fn poll( listener: EventListener, cx: &mut Self::Context )
        -> Result<(), EventListener>;
}

//------------------------------------------------------------------------------
//  Non blocking strategy for use in asynchronous code.
//------------------------------------------------------------------------------
struct NonBlocking<'a>(&'a mut ());

impl<'a> Strategy for NonBlocking<'a>
{
    type Context = Context<'a>;

    fn poll( mut listener: EventListener, cx: &mut Context<'a> )
        -> Result<(), EventListener>
    {
        match Pin::new(&mut listener).poll(cx)
        {
            Poll::Ready(()) => Ok(()),
            Poll::Pending => Err(listener),
        }
    }
}

//------------------------------------------------------------------------------
//  Blocking strategy for use in synchronous code.
//------------------------------------------------------------------------------
struct Blocking;

impl Strategy for Blocking
{
    type Context = ();

    fn poll( listener: EventListener, _cx: &mut () )
        -> Result<(), EventListener>
    {
        listener.wait();
        Ok(())
    }
}
