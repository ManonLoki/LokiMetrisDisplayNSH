// 引入被测观察者集合
use super::ActivityObservers;
use std::sync::{Arc, Mutex};

#[test]
// 未知的初始状态不会被捏造成前台事件
fn unknown_initial_state_is_not_an_invented_foreground_event() {
    let observers = Arc::new(ActivityObservers::default());
    // 记录回调收到的值
    let values = Arc::new(Mutex::new(Vec::new()));
    let output = values.clone();
    // 订阅
    let observer = observers.observe(move |value| output.lock().unwrap().push(value));
    // 初始状态未知
    assert_eq!(observers.foreground(), None);
    // 此时交付不会触发回调
    observers.deliver(observer.id());
    assert!(values.lock().unwrap().is_empty());
    // 先发布后台再发布前台，回调依次收到两个值
    observers.publish(false);
    observers.deliver(observer.id());
    observers.publish(true);
    assert_eq!(*values.lock().unwrap(), [false, true]);
}

#[test]
// 排队的初始交付不会回放过期的值
fn queued_initial_delivery_never_replays_a_stale_value() {
    let observers = Arc::new(ActivityObservers::default());
    // 订阅之前状态为前台
    observers.publish(true);
    let values = Arc::new(Mutex::new(Vec::new()));
    let output = values.clone();
    // 订阅后，在初始交付之前状态变成后台
    let observer = observers.observe(move |value| output.lock().unwrap().push(value));
    observers.publish(false);
    // 初始交付读取执行时的事实
    observers.deliver(observer.id());
    // 只收到最新值，不回放过期的前台
    assert_eq!(*values.lock().unwrap(), [false]);
    assert_eq!(observers.foreground(), Some(false));
}

#[test]
// 在另一个线程丢弃令牌会释放对应的回调
fn dropping_a_token_on_another_thread_releases_the_callback() {
    let observers = Arc::new(ActivityObservers::default());
    // 用弱引用探测回调是否被释放
    let retained = Arc::new(());
    let weak = Arc::downgrade(&retained);
    // 回调捕获了被探测的对象
    let observer = observers.observe(move |_| {
        let _ = &retained;
    });
    let id = observer.id();
    // 订阅存在时对象存活
    assert!(weak.upgrade().is_some());
    // 在另一线程丢弃令牌
    std::thread::spawn(move || drop(observer)).join().unwrap();
    // 回调随之释放
    assert!(weak.upgrade().is_none());
    // 之后发布和交付都不会调用已移除的回调
    observers.publish(true);
    observers.deliver(id);
}

#[test]
// 回调里再次发布不会死锁，也不会交付外层过期的值
fn callbacks_can_publish_again_without_deadlock_or_stale_outer_delivery() {
    let observers = Arc::new(ActivityObservers::default());
    let weak = Arc::downgrade(&observers);
    // 第一个回调：收到前台就再发布后台
    let _first = observers.observe(move |value| {
        if value {
            weak.upgrade().unwrap().publish(false);
        }
    });
    let values = Arc::new(Mutex::new(Vec::new()));
    let output = values.clone();
    // 第二个回调：记录收到的值
    let _second = observers.observe(move |value| output.lock().unwrap().push(value));
    observers.publish(true);
    // 第二个回调只收到最新的后台，不会收到外层过期的前台
    assert_eq!(*values.lock().unwrap(), [false]);
}

#[test]
// 回调可以移除下一个订阅
fn a_callback_can_remove_the_next_subscription() {
    let observers = Arc::new(ActivityObservers::default());
    let next = Arc::new(Mutex::new(None));
    let pending = next.clone();
    // 第一个回调：移除下一个订阅
    let _first = observers.observe(move |_| {
        pending.lock().unwrap().take();
    });
    // 第二个订阅一旦被调用就失败
    *next.lock().unwrap() = Some(observers.observe(|_| panic!("removed observer was called")));
    // 发布后，被移除的订阅不应被调用
    observers.publish(true);
}

#[test]
// 销毁回调时连带丢弃另一个订阅不会死锁
fn destroying_a_callback_can_drop_another_subscription_without_deadlock() {
    let observers = Arc::new(ActivityObservers::default());
    let retained = Arc::new(());
    let weak = Arc::downgrade(&retained);
    // 子订阅的回调捕获了探测对象
    let child = observers.observe(move |_| {
        let _ = &retained;
    });
    // 父订阅的回调捕获了子订阅令牌
    let parent = observers.observe(move |_| {
        let _ = &child;
    });
    let (completed, received) = std::sync::mpsc::channel();
    // 在线程里丢弃父订阅：连带释放子订阅，不能死锁
    let worker = std::thread::spawn(move || {
        drop(parent);
        completed.send(()).unwrap();
    });
    // 3 秒内必须完成
    received
        .recv_timeout(std::time::Duration::from_secs(3))
        .expect("dropping a callback deadlocked while releasing its captured subscription");
    worker.join().unwrap();
    // 子订阅的回调也已释放
    assert!(
        weak.upgrade().is_none(),
        "captured subscription retained its callback after teardown"
    );
}
