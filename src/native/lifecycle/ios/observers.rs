//! 主队列订阅分发，回调执行期间不持锁；释放 token 即移除后续交付。
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};

type Callback = dyn Fn(bool) + Send + Sync;

struct Listener {
    callback: Box<Callback>,
    last: AtomicU8,
}

#[derive(Default)]
pub(super) struct ActivityObservers {
    current: AtomicU8,
    next: AtomicU64,
    listeners: Mutex<BTreeMap<u64, Arc<Listener>>>,
}

/// 只含 Rust 弱引用，VM 可以在 engine 线程释放订阅。
pub(in crate::native) struct ActivityObserver {
    id: u64,
    observers: Weak<ActivityObservers>,
}

impl std::fmt::Debug for ActivityObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("ActivityObserver")
            .field(&self.id)
            .finish()
    }
}

impl Drop for ActivityObserver {
    fn drop(&mut self) {
        if let Some(observers) = self.observers.upgrade() {
            let removed = observers
                .listeners
                .lock()
                .expect("iOS observer lock poisoned")
                .remove(&self.id);
            // 闭包析构也可能释放另一枚 token；必须先释放订阅表的锁。
            drop(removed);
        }
    }
}

impl ActivityObserver {
    pub(super) fn id(&self) -> u64 {
        self.id
    }
}

impl ActivityObservers {
    pub(super) fn foreground(&self) -> Option<bool> {
        match self.current.load(Ordering::Acquire) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    pub(super) fn observe(
        self: &Arc<Self>,
        callback: impl Fn(bool) + Send + Sync + 'static,
    ) -> ActivityObserver {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        assert_ne!(id, u64::MAX, "iOS lifecycle observer identifiers exhausted");
        let listener = Listener {
            callback: Box::new(callback),
            last: AtomicU8::new(0),
        };
        self.listeners
            .lock()
            .expect("iOS observer lock poisoned")
            .insert(id, Arc::new(listener));
        ActivityObserver {
            id,
            observers: Arc::downgrade(self),
        }
    }

    /// 调用者在主队列串行发布；重入通知交付最新事实，不回放外层旧快照。
    pub(super) fn publish(&self, visible: bool) {
        let value = if visible { 1 } else { 2 };
        if self.current.swap(value, Ordering::AcqRel) == value {
            return;
        }
        let listeners: Vec<_> = self
            .listeners
            .lock()
            .expect("iOS observer lock poisoned")
            .keys()
            .copied()
            .collect();
        for id in listeners {
            self.deliver(id);
        }
    }

    /// 初始交付也读取执行时的事实；token 提前释放时不会保留用户闭包。
    pub(super) fn deliver(&self, id: u64) {
        let listener = self
            .listeners
            .lock()
            .expect("iOS observer lock poisoned")
            .get(&id)
            .cloned();
        if let Some(listener) = listener {
            let value = self.current.load(Ordering::Acquire);
            if value != 0 && listener.last.swap(value, Ordering::AcqRel) != value {
                (listener.callback)(value == 1);
            }
        }
    }
}

#[cfg(test)]
#[path = "observers_test.rs"]
mod ios_observers_test;
