# concurrent_queue

A concurrent multi-producer and multi-consumer queue.

There are two kinds of queues:

1. Bounded queue with limited capacity.
1. Unbounded queue with unlimited capacity.

Queeus also have the capability to get closed at any point. When closed, no 
more items can be pushed into the queue, although the remaining can still be 
append.

These features make it easy to build channels similar to std::sync::mpsc on top 
of this crate.
