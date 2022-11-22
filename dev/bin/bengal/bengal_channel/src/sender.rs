/*

    Sender

*/

use crate::Channel;

use std::sync::Arc;

pub struct Sender<T>
{
    pub(crate) channel: Arc<Channel<T>>,
}

impl<T> Sender<T>
{
    pub fn try_send( &self, msg: T ) -> Result<(), TrySendError<T>>
    {
        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TrySendError<T>
{
    Full(T),
    Closed(T),
}
