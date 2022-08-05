//! Scheduler

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::task::Wake;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use crossbeam_queue::ArrayQueue;
use spin::Mutex;
use x86_64::instructions::interrupts;

type SharedTask = Arc<Mutex<Task>>;
type TaskQueue = Arc<ArrayQueue<SharedTask>>;

pub struct Task {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + Send + 'static) -> Task {
        Task {
            future: Box::pin(future),
        }
    }

    fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

pub struct Scheduler {
    task_queue: TaskQueue,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            task_queue: Arc::new(ArrayQueue::new(128)),
        }
    }

    pub fn schedule(&self, task: Task) {
        let task = Arc::new(Mutex::new(task));
        self.task_queue.push(task).ok().expect("Task queue is full");
    }

    /// Starts execution of component on this CPU core.
    pub fn run(&self) -> ! {
        loop {
            self.run_ready_tasks();
            self.sleep_if_idle();
        }
    }

    /// Halt the current core if the task queue is empty.
    fn sleep_if_idle(&self) {
        // Deactivate interrupts to prevent race conditions
        interrupts::disable();
        if self.task_queue.is_empty() {
            interrupts::enable_and_hlt();
        } else {
            interrupts::enable();
        }
    }

    fn run_ready_tasks(&self) {
        while let Some(task) = self.task_queue.pop() {
            // TODO: optimize waker? (remove clone and from_waker)
            let waker = TaskWaker::new(task.clone(), self.task_queue.clone());
            let mut ctx = Context::from_waker(&waker);
            let mut task = task.lock();
            match task.poll(&mut ctx) {
                Poll::Ready(()) => {
                    // Task done
                }
                Poll::Pending => {
                    // Task pending...
                }
            }
        }
    }
}

pub struct TaskWaker {
    task: SharedTask,
    queue: TaskQueue,
}

impl TaskWaker {
    fn new(task: SharedTask, queue: TaskQueue) -> Waker {
        Waker::from(Arc::new(TaskWaker { task, queue }))
    }

    fn wake_task(&self) {
        self.queue
            .push(self.task.clone())
            .ok()
            .expect("Can't wake task: task queue is full");
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}
