use super::*;
use std::{sync::mpsc::Receiver, task::Wake};

#[derive(Default)]
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn service(length: usize) -> (Service, Receiver<Job>) {
    let (sender, receiver) = mpsc::sync_channel(length);
    (
        Service {
            sender,
            budget: Arc::default(),
        },
        receiver,
    )
}

#[test]
fn polling_before_and_after_completion_never_loses_a_wakeup() {
    let (service, receiver) = service(2);
    let wake = Arc::new(WakeCount::default());
    let waker = Waker::from(Arc::clone(&wake));
    let mut context = Context::from_waker(&waker);
    let mut pending = service.submit("first".into()).unwrap();
    assert!(Pin::new(&mut pending).poll(&mut context).is_pending());
    process(receiver.recv().unwrap(), |source| Ok(source.to_owned()));
    assert_eq!(wake.0.load(Ordering::SeqCst), 1);
    assert_eq!(
        Pin::new(&mut pending).poll(&mut context),
        Poll::Ready(Ok("first".into()))
    );
    let mut late = service.submit("second".into()).unwrap();
    process(receiver.recv().unwrap(), |source| Ok(source.to_owned()));
    assert_eq!(
        Pin::new(&mut late).poll(&mut context),
        Poll::Ready(Ok("second".into()))
    );
    assert_eq!(
        wake.0.load(Ordering::SeqCst),
        1,
        "unregistered consumers need no wake"
    );
}

#[test]
fn full_queue_fails_without_blocking_and_releases_its_byte_reservation() {
    let (service, receiver) = service(1);
    let pending = service.submit("queued".into()).unwrap();
    assert!(service.submit("rejected".into()).is_err());
    assert_eq!(service.budget.load(Ordering::SeqCst), 6);
    drop(pending);
    process(receiver.recv().unwrap(), |_| {
        panic!("a cancelled job must not decode")
    });
    assert_eq!(service.budget.load(Ordering::SeqCst), 0);
    drop(receiver);
    assert!(service.submit("disconnected".into()).is_err());
    assert_eq!(service.budget.load(Ordering::SeqCst), 0);
}

#[test]
fn cancellation_skips_queued_work_and_discards_running_work() {
    let (service, receiver) = service(1);
    let pending = service.submit("queued".into()).unwrap();
    let job = receiver.recv().unwrap();
    let shared = Arc::clone(&job.shared);
    drop(pending);
    process(job, |_| panic!("a cancelled queued job ran"));
    assert!(shared.completion.lock().unwrap().result.is_none());
    let pending = service.submit("running".into()).unwrap();
    let job = receiver.recv().unwrap();
    let shared = Arc::clone(&job.shared);
    process(job, |_| {
        drop(pending);
        Ok("discarded".into())
    });
    assert!(shared.completion.lock().unwrap().result.is_none());
    assert_eq!(service.budget.load(Ordering::SeqCst), 0);
}

#[test]
fn replacing_a_waker_releases_captured_resources_outside_the_completion_lock() {
    struct DropProbe {
        shared: std::sync::Weak<Shared>,
        released: Arc<AtomicBool>,
        wake_count: AtomicUsize,
    }
    impl Wake for DropProbe {
        fn wake(self: Arc<Self>) {
            self.wake_count.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl Drop for DropProbe {
        fn drop(&mut self) {
            let shared = self.shared.upgrade().unwrap();
            assert!(
                shared.completion.try_lock().is_ok(),
                "a custom waker destructor ran while locked"
            );
            self.released.store(true, Ordering::SeqCst);
        }
    }
    let (service, _receiver) = service(1);
    let mut pending = service.submit("queued".into()).unwrap();
    let released = Arc::new(AtomicBool::new(false));
    let first = Waker::from(Arc::new(DropProbe {
        shared: Arc::downgrade(&pending.0),
        released: Arc::clone(&released),
        wake_count: AtomicUsize::new(0),
    }));
    assert!(
        Pin::new(&mut pending)
            .poll(&mut Context::from_waker(&first))
            .is_pending()
    );
    drop(first);
    assert!(!released.load(Ordering::SeqCst));
    let second = Waker::from(Arc::new(WakeCount::default()));
    assert!(
        Pin::new(&mut pending)
            .poll(&mut Context::from_waker(&second))
            .is_pending()
    );
    assert!(released.load(Ordering::SeqCst));
}
