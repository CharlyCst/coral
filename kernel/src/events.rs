//! Event Sources and Dispatchers

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::pin::Pin;
use core::task::{Context, Poll};

use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use futures::stream::Stream;
use futures::task::AtomicWaker;
use futures::StreamExt;
use spin::Mutex;

use crate::kprint;
use crate::scheduler::Task;

// —————————————————————————————— Known Events —————————————————————————————— //

pub static KEYBOARD_EVENTS: StaticEventSource<u8> = StaticEventSource::new();
pub static TIMER_EVENTS: StaticEventSource<()> = StaticEventSource::new();

pub(crate) fn push_keyboard_event(scancode: u8) {
    if let Some(queue) = KEYBOARD_EVENTS.try_get() {
        queue.dispatch(scancode);
    }
}

pub(crate) fn push_timer_event() {
    if let Some(queue) = TIMER_EVENTS.try_get() {
        queue.dispatch(());
    }
}

// —————————————————————————— Static Event Source ——————————————————————————— //

pub struct StaticEventSource<T>(OnceCell<Arc<EventSource<T>>>);

impl<T> StaticEventSource<T> {
    pub const fn new() -> Self {
        Self(OnceCell::uninit())
    }

    /// Initializes the envent source.
    ///
    /// Must be called only once, panic otherwise.
    pub fn initialize(&self, source: Arc<EventSource<T>>) {
        self.0
            .try_init_once(|| source)
            .expect("Static event sources must be initialized only once");
    }

    /// Returns the underlying event source, if already initialized.
    pub fn try_get(&self) -> Option<&EventSource<T>> {
        self.0.try_get().ok().map(|inner| inner.as_ref())
    }
}

// —————————————————————————————— Event Source —————————————————————————————— //

/// An event source.
///
/// The events send to the source are asyncronously dispatched to a potentially dynamic set of
/// listeners by the the associated `EventDispatcher`.
pub struct EventSource<T> {
    queue: ArrayQueue<T>,
    waker: AtomicWaker,
}

impl<T> EventSource<T> {
    fn new(queue: ArrayQueue<T>) -> Self {
        Self {
            queue,
            waker: AtomicWaker::new(),
        }
    }

    /// Pushes an event to the queue and wake the corresponding event source.
    pub fn dispatch(&self, item: T) {
        self.queue.push(item);
        self.waker.wake();
    }
}

struct SourceStream<T> {
    source: Arc<EventSource<T>>,
}

impl<T> SourceStream<T> {
    fn new(source: Arc<EventSource<T>>) -> Pin<Box<Self>> {
        Box::pin(Self { source })
    }
}

impl<T> Stream for SourceStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(item) = self.source.queue.pop() {
            return Poll::Ready(Some(item));
        }

        self.source.waker.register(ctx.waker());
        // Check again in case an item was received asynchronously
        match self.source.queue.pop() {
            Some(item) => Poll::Ready(Some(item)),
            None => Poll::Pending,
        }
    }
}

// ———————————————————————————— Event Dispatcher ———————————————————————————— //

/// An event dispatched.
///
/// A dispatcher is connected to an event source, and can be scheduled to asyncronously wait on new
/// events and dispatch them to listeners.
pub struct EventDispatcher<T> {
    listeners: Mutex<Vec<()>>,
    source: Arc<EventSource<T>>,
}

impl<T> EventDispatcher<T> {
    /// Creates a new event dispatcher with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let queue = ArrayQueue::new(capacity);
        let source = EventSource::new(queue);
        EventDispatcher {
            listeners: Mutex::new(Vec::new()),
            source: Arc::new(source),
        }
    }

    pub fn source(&self) -> &Arc<EventSource<T>> {
        &self.source
    }

    async fn as_promise(self: Arc<Self>, mut stream: Pin<Box<SourceStream<T>>>) {
        while let Some(_item) = stream.next().await {
            // Just print something for now
            kprint!(".");
        }
    }
}

impl<T> EventDispatcher<T>
where
    T: 'static,
{
    /// Creates a dispatch task.
    ///
    /// The task asynchronously wait for event and dispatch them to the listeners.
    pub fn dispatch(self: Arc<Self>) -> Task {
        let stream = SourceStream::new(self.source.clone());
        Task::new(self.clone().as_promise(stream))
    }
}
