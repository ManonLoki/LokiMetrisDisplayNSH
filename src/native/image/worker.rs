//! A bounded decoder worker keeps image preparation off every NSH engine thread.
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    task::{Context, Poll, Waker},
};

/// One running job and at most 32 waiting jobs; the byte budget includes
/// the running job until its source bytes have been released.
const QUEUE_LENGTH: usize = 32;
const QUEUED_BYTES: usize = 48 * 1024 * 1024;

#[derive(Default)]
struct Completion {
    result: Option<Result<String, String>>,
    waker: Option<Waker>,
}

#[derive(Default)]
struct Shared {
    cancelled: AtomicBool,
    completion: Mutex<Completion>,
}

struct Pending(Arc<Shared>);

impl Future for Pending {
    type Output = Result<String, String>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let next_waker = context.waker().clone();
        let (result, previous_waker) = {
            let mut completion = self.0.completion.lock().expect("GIF completion poisoned");
            if let Some(result) = completion.result.take() {
                (Poll::Ready(result), completion.waker.take())
            } else {
                // Registration and completion share the lock, while custom
                // waker clone/drop code always runs outside it.
                (Poll::Pending, completion.waker.replace(next_waker))
            }
        };
        drop(previous_waker);
        result
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.0.cancelled.store(true, Ordering::Release);
        let waker = self
            .0
            .completion
            .lock()
            .expect("GIF completion poisoned")
            .waker
            .take();
        drop(waker);
    }
}

struct Reservation {
    bytes: usize,
    budget: Arc<AtomicUsize>,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

struct Job {
    source: String,
    shared: Arc<Shared>,
    _reservation: Reservation,
}

struct Service {
    sender: SyncSender<Job>,
    budget: Arc<AtomicUsize>,
}

impl Service {
    fn start() -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_LENGTH);
        std::thread::Builder::new()
            .name("loki-gif-frames".into())
            .spawn(move || {
                for job in receiver {
                    process(job, super::prepare);
                }
            })
            .map_err(|_| "GIF解码线程无法启动")?;
        Ok(Self {
            sender,
            budget: Arc::default(),
        })
    }

    fn submit(&self, source: String) -> Result<Pending, String> {
        let bytes = source.len();
        if bytes > super::MAX_SOURCE.div_ceil(3) * 4 + 64 {
            return Err("GIF来源超过长度限额".into());
        }
        self.budget
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= QUEUED_BYTES)
            })
            .map_err(|_| "GIF解码队列字节限额已满")?;
        let shared = Arc::new(Shared::default());
        let job = Job {
            source,
            shared: Arc::clone(&shared),
            _reservation: Reservation {
                bytes,
                budget: Arc::clone(&self.budget),
            },
        };
        self.sender
            .try_send(job)
            .map_err(|_| "GIF解码队列已满或线程不可用")?;
        Ok(Pending(shared))
    }
}

fn process(job: Job, prepare: impl FnOnce(&str) -> Result<String, String>) {
    if job.shared.cancelled.load(Ordering::Acquire) {
        return;
    }
    let result = prepare(&job.source);
    let waker = {
        let mut completion = job
            .shared
            .completion
            .lock()
            .expect("GIF completion poisoned");
        if job.shared.cancelled.load(Ordering::Acquire) {
            return;
        }
        completion.result = Some(result);
        completion.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

pub(super) async fn prepare(source: String) -> Result<String, String> {
    static SERVICE: OnceLock<Result<Service, String>> = OnceLock::new();
    let service = SERVICE
        .get_or_init(Service::start)
        .as_ref()
        .map_err(Clone::clone)?;
    service.submit(source)?.await
}

#[cfg(test)]
#[path = "worker_test.rs"]
mod worker_test;
