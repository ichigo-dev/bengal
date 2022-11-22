# Bengal


## What is `Stream`

- [futures_core::stream::Stream](https://docs.rs/futures-core/0.3.25/futures_core/stream/trait.Stream.html)

> A stream of values produced asynchronously.
>
> If Future<Output = T> is an asynchronous version of T, then Stream<Item = T> 
> is an asynchronous version of Iterator<Item = T>. A stream represents a 
> sequence of value-producing events that occur asynchronously to the caller.
>
> The trait is modeled after Future, but allows poll_next to be called even 
> after a value has been produced, yielding None once the stream has been fully 
> exhausted.

One common example of a Stream is the Receiver for the channel type from the 
futures crate. It will yield Some(val) every time a value is sent from the 
Sender end, and will yield None once the Sender has been dropped and all 
pending messages have been received.
