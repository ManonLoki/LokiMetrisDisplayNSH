// 引入被测模块
use super::*;
use std::{sync::mpsc::Receiver, task::Wake};

#[derive(Default)]
// 统计被唤醒次数
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

// 构造指定字节预算的服务与任务接收端
fn service(length: usize) -> (Service, Receiver<Job>) {
    let (sender, receiver) = mpsc::sync_channel(length);
    (
        // 不启动工作线程，由测试手动处理任务
        Service {
            sender,
            budget: Arc::default(),
        },
        receiver,
    )
}

#[test]
// 完成前后轮询都不会丢失唤醒
fn polling_before_and_after_completion_never_loses_a_wakeup() {
    let (service, receiver) = service(2);
    let wake = Arc::new(WakeCount::default());
    let waker = Waker::from(Arc::clone(&wake));
    let mut context = Context::from_waker(&waker);
    // 提交后先轮询：仍在等待，此时登记了唤醒器
    let mut pending = service.submit("first".into()).unwrap();
    assert!(Pin::new(&mut pending).poll(&mut context).is_pending());
    // 工作线程处理：应唤醒一次
    process(receiver.recv().unwrap(), |source| Ok(source.to_owned()));
    assert_eq!(wake.0.load(Ordering::SeqCst), 1);
    // 再轮询得到结果
    assert_eq!(
        Pin::new(&mut pending).poll(&mut context),
        Poll::Ready(Ok("first".into()))
    );
    // 第二个任务在被轮询之前就已完成
    let mut late = service.submit("second".into()).unwrap();
    process(receiver.recv().unwrap(), |source| Ok(source.to_owned()));
    // 直接得到结果
    assert_eq!(
        Pin::new(&mut late).poll(&mut context),
        Poll::Ready(Ok("second".into()))
    );
    // 没登记唤醒器的消费者不需要被唤醒
    assert_eq!(
        wake.0.load(Ordering::SeqCst),
        1,
        "unregistered consumers need no wake"
    );
}

#[test]
// 队列已满时立即失败而不阻塞，并归还占用的字节预算
fn full_queue_fails_without_blocking_and_releases_its_byte_reservation() {
    // 队列只有一格
    let (service, receiver) = service(1);
    let pending = service.submit("queued".into()).unwrap();
    // 第二个任务立即被拒绝，不阻塞
    assert!(service.submit("rejected".into()).is_err());
    // 被拒的任务不占预算，只有排队任务的 6 字节
    assert_eq!(service.budget.load(Ordering::SeqCst), 6);
    // 提交方放弃后，处理该任务时不解码
    drop(pending);
    process(receiver.recv().unwrap(), |_| {
        panic!("a cancelled job must not decode")
    });
    // 预算归还
    assert_eq!(service.budget.load(Ordering::SeqCst), 0);
    // 线程不可用时提交也失败，且预算不泄漏
    drop(receiver);
    assert!(service.submit("disconnected".into()).is_err());
    assert_eq!(service.budget.load(Ordering::SeqCst), 0);
}

#[test]
// 取消会跳过排队的任务，并丢弃正在运行的任务结果
fn cancellation_skips_queued_work_and_discards_running_work() {
    let (service, receiver) = service(1);
    // 排队中被取消：任务被跳过
    let pending = service.submit("queued".into()).unwrap();
    let job = receiver.recv().unwrap();
    let shared = Arc::clone(&job.shared);
    drop(pending);
    process(job, |_| panic!("a cancelled queued job ran"));
    // 没有留下结果
    assert!(shared.completion.lock().unwrap().result.is_none());
    // 运行中被取消：结果被丢弃
    let pending = service.submit("running".into()).unwrap();
    let job = receiver.recv().unwrap();
    let shared = Arc::clone(&job.shared);
    process(job, |_| {
        // 在解码过程中放弃
        drop(pending);
        Ok("discarded".into())
    });
    assert!(shared.completion.lock().unwrap().result.is_none());
    assert_eq!(service.budget.load(Ordering::SeqCst), 0);
}

#[test]
// 替换唤醒器时，在完成锁之外释放其捕获的资源
fn replacing_a_waker_releases_captured_resources_outside_the_completion_lock() {
    // 探针唤醒器：析构时检查完成锁是否空闲
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
    // 探针的析构
    impl Drop for DropProbe {
        // 销毁时
        fn drop(&mut self) {
            // 升级共享状态引用
            let shared = self.shared.upgrade().unwrap();
            // 析构时不能持有完成锁
            assert!(
                shared.completion.try_lock().is_ok(),
                "a custom waker destructor ran while locked"
            );
            self.released.store(true, Ordering::SeqCst);
        }
    }
    let (service, _receiver) = service(1);
    // 提交排队任务
    let mut pending = service.submit("queued".into()).unwrap();
    // 释放标记
    let released = Arc::new(AtomicBool::new(false));
    // 先用探针唤醒器轮询
    let first = Waker::from(Arc::new(DropProbe {
        shared: Arc::downgrade(&pending.0),
        released: Arc::clone(&released),
        wake_count: AtomicUsize::new(0),
    }));
    assert!(
        Pin::new(&mut pending)
            .poll(&mut Context::from_waker(&first))
            // 仍在等待
            .is_pending()
    );
    // 外部丢弃后，探针仍被保存的那份持有，尚未释放
    drop(first);
    assert!(!released.load(Ordering::SeqCst));
    // 换另一个唤醒器轮询：旧的在锁外被释放
    let second = Waker::from(Arc::new(WakeCount::default()));
    assert!(
        Pin::new(&mut pending)
            .poll(&mut Context::from_waker(&second))
            .is_pending()
    );
    assert!(released.load(Ordering::SeqCst));
}
