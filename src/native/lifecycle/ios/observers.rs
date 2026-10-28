//! 主队列订阅分发，回调执行期间不持锁；释放 token 即移除后续交付。
// 引入集合与同步原语
use std::{
    // 有序映射
    collections::BTreeMap,
    // 同步原语
    sync::{
        // 原子引用计数、互斥锁与弱引用
        Arc,
        // 互斥锁
        Mutex,
        // 弱引用
        Weak,
        // 原子类型与内存序
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};

// 回调类型：参数为是否可见
type Callback = dyn Fn(bool) + Send + Sync;

// 订阅者：回调与它最近收到的值
struct Listener {
    // 回调
    callback: Box<Callback>,
    // 最近一次通知的值
    last: AtomicU8,
}

#[derive(Default)]
// 观察者集合：主队列订阅分发，回调执行期间不持锁
pub(super) struct ActivityObservers {
    // 当前状态：0 未知，1 前台，2 后台
    current: AtomicU8,
    // 下一个订阅编号
    next: AtomicU64,
    // 订阅表
    listeners: Mutex<BTreeMap<u64, Arc<Listener>>>,
}

/// 只含 Rust 弱引用，VM 可以在 engine 线程释放订阅。
// 订阅令牌，只含 Rust 弱引用，VM 可以在引擎线程释放订阅
pub(in crate::native) struct ActivityObserver {
    // 标识
    id: u64,
    // 所属集合
    observers: Weak<ActivityObservers>,
}

// 调试输出的实现
impl std::fmt::Debug for ActivityObserver {
    // 格式化
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 格式化器
        formatter
            // 调试元组
            .debug_tuple("ActivityObserver")
            // 字段为标识
            .field(&self.id)
            // 结束
            .finish()
    }
}

// 释放令牌即移除后续交付
impl Drop for ActivityObserver {
    // 销毁时
    fn drop(&mut self) {
        // 集合仍然存在
        if let Some(observers) = self.observers.upgrade() {
            // 移除自己的监听者
            let removed = observers
                // 监听者表
                .listeners
                // 加锁
                .lock()
                // 锁中毒则崩溃
                .expect("iOS observer lock poisoned")
                // 移除自己
                .remove(&self.id);
            // 闭包析构也可能释放另一枚 token；必须先释放订阅表的锁。
            // 闭包析构也可能释放另一枚令牌，所以必须先释放订阅表的锁
            drop(removed);
        }
    }
}

// 观察者的方法
impl ActivityObserver {
    // 读取标识
    pub(super) fn id(&self) -> u64 {
        // 返回标识
        self.id
    }
}

// 观察者集合的方法
impl ActivityObservers {
    // 当前前台状态；未知为 None
    pub(super) fn foreground(&self) -> Option<bool> {
        // 读取当前值
        match self.current.load(Ordering::Acquire) {
            // 1 表示前台
            1 => Some(true),
            // 2 表示后台
            2 => Some(false),
            // 其余未知
            _ => None,
        }
    }

    // 新增订阅
    pub(super) fn observe(
        // 自身
        self: &Arc<Self>,
        // 回调
        callback: impl Fn(bool) + Send + Sync + 'static,
        // 返回观察者
    ) -> ActivityObserver {
        // 分配新标识
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        // 标识不能耗尽
        assert_ne!(id, u64::MAX, "iOS lifecycle observer identifiers exhausted");
        // 构造监听者
        let listener = Listener {
            // 回调装箱
            callback: Box::new(callback),
            // 最近一次通知的值初始为 0
            last: AtomicU8::new(0),
        };
        // 监听者表
        self.listeners
            // 加锁
            .lock()
            // 锁中毒则崩溃
            .expect("iOS observer lock poisoned")
            // 登记监听者
            .insert(id, Arc::new(listener));
        // 返回观察者
        ActivityObserver {
            // 标识
            id,
            // 弱引用集合
            observers: Arc::downgrade(self),
        }
    }

    /// 调用者在主队列串行发布；重入通知交付最新事实，不回放外层旧快照。
    // 调用者在主队列串行发布；重入通知交付最新事实，不回放外层的旧快照
    pub(super) fn publish(&self, visible: bool) {
        // 1 表示前台，2 表示后台
        let value = if visible { 1 } else { 2 };
        // 值未变则不通知
        if self.current.swap(value, Ordering::AcqRel) == value {
            // 直接返回
            return;
        }
        // 先复制编号，回调时再查表
        let listeners: Vec<_> = self
            // 监听者表
            .listeners
            // 加锁
            .lock()
            // 锁中毒则崩溃
            .expect("iOS observer lock poisoned")
            // 全部标识
            .keys()
            // 复制
            .copied()
            // 收集为列表
            .collect();
        // 逐个监听者
        for id in listeners {
            // 投递给该监听者
            self.deliver(id);
        }
    }

    /// 初始交付也读取执行时的事实；token 提前释放时不会保留用户闭包。
    // 初始交付也读取执行时的事实；令牌提前释放时不会保留用户闭包
    pub(super) fn deliver(&self, id: u64) {
        // 取出该监听者
        let listener = self
            // 监听者表
            .listeners
            // 加锁
            .lock()
            // 锁中毒则崩溃
            .expect("iOS observer lock poisoned")
            // 按标识取
            .get(&id)
            // 复制
            .cloned();
        // 仍然存在
        if let Some(listener) = listener {
            // 读取当前值
            let value = self.current.load(Ordering::Acquire);
            // 只有状态已知且与上次不同才回调
            if value != 0 && listener.last.swap(value, Ordering::AcqRel) != value {
                // 回调并传入是否前台
                (listener.callback)(value == 1);
            }
        }
    }
}

// 测试模块
#[cfg(test)]
#[path = "observers_test.rs"]
mod ios_observers_test;
