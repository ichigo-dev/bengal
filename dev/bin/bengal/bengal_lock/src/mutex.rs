/*

    Mutex

*/

use std::cell::UnsafeCell;
use std::fmt;
use std::future::Future;
use std::ops::{ Deref, DerefMut };
use std::process;
use std::sync::Arc;
use std::sync::atomic::{ AtomicUsize, Ordering };

#[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
use std::time::{ Duration, Instant };

use event_listener::Event;

//------------------------------------------------------------------------------
//  It is a kind of exclusive control and synchronization mechanism for 
//  ensuring atomicity in the critical section.
//------------------------------------------------------------------------------
pub struct Mutex<T: ?Sized>
{
    //  Current state of the mutex.
    //
    //  0: Locks can be acquired.
    //  1: Now acquireing lock.
    //  (The number of starved lock operations)
    //  2: Locks can be acquired.
    //  3 or more even numbers: Lock is available.
    //  3 or more odd numbers: Lock is held by someone.
    state: AtomicUsize,

    //  Event notified when the mutex is unlocked.
    lock_event: Event,

    //  The value inside the mutex.
    data: UnsafeCell<T>,
}

unsafe impl<T: Send + ?Sized> Send for Mutex<T> {}
unsafe impl<T: Sync + ?Sized> Sync for Mutex<T> {}

impl<T> Mutex<T>
{
    //--------------------------------------------------------------------------
    //  Creates a new async mutex.
    //--------------------------------------------------------------------------
    pub const fn new( data: T ) -> Mutex<T>
    {
        Mutex
        {
            state: AtomicUsize::new(0),
            lock_event: Event::new(),
            data: UnsafeCell::new(data),
        }
    }

    //--------------------------------------------------------------------------
    //  Consumes the mutex and returning the underlying data.
    //--------------------------------------------------------------------------
    pub fn into_inner( self ) -> T
    {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Mutex<T>
{
    //--------------------------------------------------------------------------
    //  Acquires a lock on the data.
    //--------------------------------------------------------------------------
    #[inline]
    pub async fn lock( &self ) -> MutexGuard<'_, T>
    {
        if let Some(guard) = self.try_lock()
        {
            return guard;
        }

        self.acquire_slow().await;
        MutexGuard(self)
    }

    //--------------------------------------------------------------------------
    //  Attempts to acquire a lock.
    //--------------------------------------------------------------------------
    #[inline]
    pub fn try_lock( &self ) -> Option<MutexGuard<'_, T>>
    {
        if self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Acquire)
            .is_ok()
        {
            Some(MutexGuard(self))
        }
        else
        {
            None
        }
    }

    //--------------------------------------------------------------------------
    //  Acquires a lock on the data and clones a reference to it.
    //--------------------------------------------------------------------------
    #[inline]
    pub fn lock_arc( self: &Arc<Self> )
        -> impl Future<Output = MutexGuardArc<T>>
    {
        self.clone().lock_arc_impl()
    }

    async fn lock_arc_impl( self: Arc<Self> ) -> MutexGuardArc<T>
    {
        if let Some(guard) = self.try_lock_arc()
        {
            return guard;
        }

        self.acquire_slow().await;
        MutexGuardArc(self)
    }

    //--------------------------------------------------------------------------
    //  Attempts to acquire a lock.
    //--------------------------------------------------------------------------
    #[inline]
    pub fn try_lock_arc( self: &Arc<Self> ) -> Option<MutexGuardArc<T>>
    {
        if self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Acquire)
            .is_ok()
        {
            Some(MutexGuardArc(self.clone()))
        }
        else
        {
            None
        }
    }

    //--------------------------------------------------------------------------
    //  Get a mutable reference to the underlying data.
    //--------------------------------------------------------------------------
    pub fn get_mut( &mut self ) -> &mut T
    {
        unsafe { &mut *self.data.get() }
    }

    //--------------------------------------------------------------------------
    //  Slow path for acquireing the mutex.
    //--------------------------------------------------------------------------
    #[cold]
    async fn acquire_slow( &self )
    {
        //  Get the current time.
        #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
        let start = Instant::now();

        //  Repeat the attempt until a lock is obtained. If starvation has 
        //  occurred due to other locking operations, proceed to the next step.
        loop
        {
            //  Start listening for events.
            let listener = self.lock_event.listen();

            //  starvation means the number of missing lock operations.
            //  Try locking if nobody is being starved.
            match self
                .state
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Acquire)
                .unwrap_or_else(|x| x)
            {
                //  Lock acquired.
                0 => return,

                //  Lock is already held and nobody is starved.
                1 => {},

                //  Somebody is starved.
                _ => break,
            }

            //  Wait for a notification.
            listener.await;

            match self
                .state
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Acquire)
                .unwrap_or_else(|x| x)
            {
                //  Lock acquired.
                0 => return,

                //  Lock is held and nobody is starved.
                1 => {},

                //  Somebody is starved.
                _ =>
                {
                    //  In this case, notificate event because starvation has 
                    //  probably occurred.
                    self.lock_event.notify(1);
                    break;
                },
            }

            #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
            if start.elapsed() > Duration::from_micros(500)
            {
                break;
            }
        }

        //  Increment the number of starved lock operations.
        if self.state.fetch_add(2, Ordering::Release) > (usize::MAX / 2)
        {
            process::abort();
        }

        //  Decrement the counter when exiting this function.
        let _call = CallOnDrop(||
        {
            self.state.fetch_sub(2, Ordering::Release);
        });

        loop
        {
            //  Start listening for events.
            let listener = self.lock_event.listen();

            //  Try locking if nobody is being starved.
            match self
                .state
                .compare_exchange(2, 2 | 1, Ordering::Acquire, Ordering::Acquire)
                .unwrap_or_else(|x| x)
            {
                //  Lock acquired.
                2 => return,

                //  Lock is held by someone.
                s if s % 2 == 1 => {},

                //  Lock is available.
                _ =>
                {
                    self.lock_event.notify(1);
                },
            }

            //  Wait for a notification.
            listener.await;

            //  Try acquiring the lock without waiting for others.
            if self.state.fetch_or(1, Ordering::Acquire) % 2 == 0
            {
                return;
            }
        }
    }
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for Mutex<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when debugging.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        struct Locked;

        impl fmt::Debug for Locked
        {
            fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
            {
                f.write_str("<locked>")
            }
        }

        match self.try_lock()
        {
            None =>
            {
                f.debug_struct("Mutex").field("data", &Locked).finish()
            },
            Some(guard) =>
            {
                f.debug_struct("Mutex").field("data", &&*guard).finish()
            },
        }
    }
}

impl<T> From<T> for Mutex<T>
{
    //--------------------------------------------------------------------------
    //  Converts to this type from the input type.
    //--------------------------------------------------------------------------
    fn from( val: T ) -> Mutex<T>
    {
        Mutex::new(val)
    }
}

impl<T: Default + ?Sized> Default for Mutex<T>
{
    //--------------------------------------------------------------------------
    //  Returns the default value for this type.
    //--------------------------------------------------------------------------
    fn default() -> Mutex<T>
    {
        Mutex::new(Default::default())
    }
}

//------------------------------------------------------------------------------
//  An RAII implementation of a “scoped lock” of a mutex. When this structure 
//  is dropped (falls out of scope), the lock will be unlocked.
//------------------------------------------------------------------------------
pub struct MutexGuard<'a, T: ?Sized>(&'a Mutex<T>);

unsafe impl<T: Send + ?Sized> Send for MutexGuard<'_, T> {}
unsafe impl<T: Sync + ?Sized> Sync for MutexGuard<'_, T> {}

impl<'a, T: ?Sized> MutexGuard<'a, T>
{
    //--------------------------------------------------------------------------
    //  Returns a reference to the mutex.
    //--------------------------------------------------------------------------
    pub fn source( guard: &MutexGuard<'a, T> ) -> &'a Mutex<T>
    {
        guard.0
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T>
{
    //--------------------------------------------------------------------------
    //  Executes the destructor for this type.
    //--------------------------------------------------------------------------
    fn drop( &mut self )
    {
        self.0.state.fetch_sub(1, Ordering::Release);
        self.0.lock_event.notify(1);
    }
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for MutexGuard<'_, T>
{
    //--------------------------------------------------------------------------
    //  Formats output when debugging.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: fmt::Display + ?Sized> fmt::Display for MutexGuard<'_, T>
{
    //--------------------------------------------------------------------------
    //  Formats output when displaying.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        (**self).fmt(f)
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T>
{
    type Target = T;

    //--------------------------------------------------------------------------
    //  Dereferences the value.
    //--------------------------------------------------------------------------
    fn deref( &self ) -> &T
    {
        unsafe { &*self.0.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T>
{
    //--------------------------------------------------------------------------
    //  Mutably dereferences the value.
    //--------------------------------------------------------------------------
    fn deref_mut( &mut self ) -> &mut T
    {
        unsafe { &mut *self.0.data.get() }
    }
}

//------------------------------------------------------------------------------
//  An RAII implementation of a “scoped lock” of a mutex. When this structure 
//  is dropped (falls out of scope), the lock will be unlocked.
//------------------------------------------------------------------------------
pub struct MutexGuardArc<T: ?Sized>(Arc<Mutex<T>>);

unsafe impl<T: Send + ?Sized> Send for MutexGuardArc<T> {}
unsafe impl<T: Sync + ?Sized> Sync for MutexGuardArc<T> {}

impl<T: ?Sized> MutexGuardArc<T>
{
    //--------------------------------------------------------------------------
    //  Returns a reference to the mutex.
    //--------------------------------------------------------------------------
    pub fn source( guard: &MutexGuardArc<T> ) -> &Arc<Mutex<T>>
    {
        &guard.0
    }
}

impl<T: ?Sized> Drop for MutexGuardArc<T>
{
    //--------------------------------------------------------------------------
    //  Executes the destructor for this type.
    //--------------------------------------------------------------------------
    fn drop( &mut self )
    {
        self.0.state.fetch_sub(1, Ordering::Release);
        self.0.lock_event.notify(1);
    }
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for MutexGuardArc<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when debugging.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: fmt::Display + ?Sized> fmt::Display for MutexGuardArc<T>
{
    //--------------------------------------------------------------------------
    //  Formats output when displaying.
    //--------------------------------------------------------------------------
    fn fmt( &self, f: &mut fmt::Formatter<'_> ) -> fmt::Result
    {
        (**self).fmt(f)
    }
}

impl<T: ?Sized> Deref for MutexGuardArc<T>
{
    type Target = T;

    //--------------------------------------------------------------------------
    //  Dereferences the value.
    //--------------------------------------------------------------------------
    fn deref( &self ) -> &T
    {
        unsafe { &*self.0.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuardArc<T>
{
    //--------------------------------------------------------------------------
    //  Mutably dereferences the value.
    //--------------------------------------------------------------------------
    fn deref_mut( &mut self ) -> &mut T
    {
        unsafe { &mut *self.0.data.get() }
    }
}

//------------------------------------------------------------------------------
//  Calles a function when dropped
//------------------------------------------------------------------------------
struct CallOnDrop<F: Fn()>(F);

impl<F: Fn()> Drop for CallOnDrop<F>
{
    //--------------------------------------------------------------------------
    //  Executes the destructor for this type.
    //--------------------------------------------------------------------------
    fn drop( &mut self )
    {
        (self.0)();
    }
}
