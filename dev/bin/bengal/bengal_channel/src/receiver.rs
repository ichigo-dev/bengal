/*

    Receiver

*/

use crate::Channel;

use std::sync::Arc;

use event_listener::EventListener;

pub struct Receiver<T>
{
    pub(crate) channel: Arc<Channel<T>>,
    pub(crate) listener: Option<EventListener>,
}
