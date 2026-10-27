use super::ActivityObservers;
use std::sync::{Arc, Mutex};

#[test]
fn unknown_initial_state_is_not_an_invented_foreground_event() {
    let observers = Arc::new(ActivityObservers::default());
    let values = Arc::new(Mutex::new(Vec::new()));
    let output = values.clone();
    let observer = observers.observe(move |value| output.lock().unwrap().push(value));
    assert_eq!(observers.foreground(), None);
    observers.deliver(observer.id());
    assert!(values.lock().unwrap().is_empty());
    observers.publish(false);
    observers.deliver(observer.id());
    observers.publish(true);
    assert_eq!(*values.lock().unwrap(), [false, true]);
}

#[test]
fn queued_initial_delivery_never_replays_a_stale_value() {
    let observers = Arc::new(ActivityObservers::default());
    observers.publish(true);
    let values = Arc::new(Mutex::new(Vec::new()));
    let output = values.clone();
    let observer = observers.observe(move |value| output.lock().unwrap().push(value));
    observers.publish(false);
    observers.deliver(observer.id());
    assert_eq!(*values.lock().unwrap(), [false]);
    assert_eq!(observers.foreground(), Some(false));
}

#[test]
fn dropping_a_token_on_another_thread_releases_the_callback() {
    let observers = Arc::new(ActivityObservers::default());
    let retained = Arc::new(());
    let weak = Arc::downgrade(&retained);
    let observer = observers.observe(move |_| {
        let _ = &retained;
    });
    let id = observer.id();
    assert!(weak.upgrade().is_some());
    std::thread::spawn(move || drop(observer)).join().unwrap();
    assert!(weak.upgrade().is_none());
    observers.publish(true);
    observers.deliver(id);
}

#[test]
fn callbacks_can_publish_again_without_deadlock_or_stale_outer_delivery() {
    let observers = Arc::new(ActivityObservers::default());
    let weak = Arc::downgrade(&observers);
    let _first = observers.observe(move |value| {
        if value {
            weak.upgrade().unwrap().publish(false);
        }
    });
    let values = Arc::new(Mutex::new(Vec::new()));
    let output = values.clone();
    let _second = observers.observe(move |value| output.lock().unwrap().push(value));
    observers.publish(true);
    assert_eq!(*values.lock().unwrap(), [false]);
}

#[test]
fn a_callback_can_remove_the_next_subscription() {
    let observers = Arc::new(ActivityObservers::default());
    let next = Arc::new(Mutex::new(None));
    let pending = next.clone();
    let _first = observers.observe(move |_| {
        pending.lock().unwrap().take();
    });
    *next.lock().unwrap() = Some(observers.observe(|_| panic!("removed observer was called")));
    observers.publish(true);
}

#[test]
fn destroying_a_callback_can_drop_another_subscription_without_deadlock() {
    let observers = Arc::new(ActivityObservers::default());
    let retained = Arc::new(());
    let weak = Arc::downgrade(&retained);
    let child = observers.observe(move |_| {
        let _ = &retained;
    });
    let parent = observers.observe(move |_| {
        let _ = &child;
    });
    let (completed, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        drop(parent);
        completed.send(()).unwrap();
    });
    received
        .recv_timeout(std::time::Duration::from_secs(3))
        .expect("dropping a callback deadlocked while releasing its captured subscription");
    worker.join().unwrap();
    assert!(
        weak.upgrade().is_none(),
        "captured subscription retained its callback after teardown"
    );
}
