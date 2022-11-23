/*

    Error

*/

use std::fmt;
use std::error;

//------------------------------------------------------------------------------
//  An error when sending a message.
//------------------------------------------------------------------------------
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SendError<T>(pub T);

impl<T> SendError<T>
{
    //--------------------------------------------------------------------------
    //  Unwraps the message that couldn't be sent.
    //--------------------------------------------------------------------------
    pub fn into_inner( self ) -> T
    {
        self.0
    }
}

impl<T> error::Error for SendError<T> {}

impl<T> fmt::Debug for SendError<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when debugging.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        write!(f, "SendError(..)")
    }
}

impl<T> fmt::Display for SendError<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when displaying.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        write!(f, "sending into a closed channel")
    }
}

//------------------------------------------------------------------------------
//  An error when trying to send a message.
//------------------------------------------------------------------------------
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TrySendError<T>
{
    Full(T),
    Closed(T),
}

impl<T> TrySendError<T>
{
    //--------------------------------------------------------------------------
    //  Unwraps the message that couldn't be sent.
    //--------------------------------------------------------------------------
    pub fn into_inner( self ) -> T
    {
        match self
        {
            TrySendError::Full(t) => t,
            TrySendError::Closed(t) => t,
        }
    }

    //--------------------------------------------------------------------------
    //  Checks if message queue is full.
    //--------------------------------------------------------------------------
    pub fn is_full( &self ) -> bool
    {
        match self
        {
            TrySendError::Full(_) => true,
            TrySendError::Closed(_) => false,
        }
    }

    //--------------------------------------------------------------------------
    //  Checks if message queue is closed.
    //--------------------------------------------------------------------------
    pub fn is_closed( &self ) -> bool
    {
        match self
        {
            TrySendError::Full(_) => false,
            TrySendError::Closed(_) => true,
        }
    }
}

impl<T> error::Error for TrySendError<T> {}

impl<T> fmt::Debug for TrySendError<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when debugging.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        match *self
        {
            TrySendError::Full(..) => write!(f, "Full(..)"),
            TrySendError::Closed(..) => write!(f, "Closed(..)"),
        }
    }
}

impl<T> fmt::Display for TrySendError<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when displaying.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        match *self
        {
            TrySendError::Full(..) =>
            {
                write!(f, "sending into a full channel")
            },
            TrySendError::Closed(..) =>
            {
                write!(f, "sending into a closed channel")
            },
        }
    }
}

//------------------------------------------------------------------------------
//  An error when receiving a message.
//------------------------------------------------------------------------------
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct RecvError;

impl error::Error for RecvError {}

impl fmt::Display for RecvError
{
    //--------------------------------------------------------------------------
    //  Formats output when displaying.
    //--------------------------------------------------------------------------
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        write!(f, "receiving from an empty and closed channel")
    }
}

//------------------------------------------------------------------------------
//  An error when trying to receive a message.
//------------------------------------------------------------------------------
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TryRecvError
{
    Empty,
    Closed,
}

impl TryRecvError
{
    //--------------------------------------------------------------------------
    //  Checks if message queue is empty.
    //--------------------------------------------------------------------------
    pub fn is_empty( &self ) -> bool
    {
        match self
        {
            TryRecvError::Empty => true,
            TryRecvError::Closed => false,
        }
    }

    //--------------------------------------------------------------------------
    //  Checks if message queue is closed.
    //--------------------------------------------------------------------------
    pub fn is_closed( &self ) -> bool
    {
        match self
        {
            TryRecvError::Empty => false,
            TryRecvError::Closed => true,
        }
    }
}

impl error::Error for TryRecvError {}

impl fmt::Display for TryRecvError
{
    //--------------------------------------------------------------------------
    //  Formats output when displaying.
    //--------------------------------------------------------------------------
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        match *self
        {
            TryRecvError::Empty =>
            {
                write!(f, "receiving from an empty channel")
            },
            TryRecvError::Closed =>
            {
                write!(f, "receiving from an empty and closed channel")
            },
        }
    }
}
