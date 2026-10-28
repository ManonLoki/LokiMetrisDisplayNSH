// 引入被测模块
use super::*;
use std::cell::RefCell;

// 已保存的身份
const OLD: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
// 新生成的身份
const NEW: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

#[test]
// 已有身份时不需要也不会读取配对记录
fn an_existing_identity_does_not_need_or_read_pairing_records() {
    // 读取闭包返回已有身份；写入和生成闭包被调用就会 panic
    let result = load(
        || Ok(Some(OLD.into())),
        |_| panic!("must not overwrite identity"),
        || panic!("must not replace identity"),
    );
    // 直接返回已有身份
    assert_eq!(result.unwrap(), OLD);
}

#[test]
// 身份缺失时先生成并保存，再返回
fn a_missing_identity_is_generated_and_saved_before_returning() {
    // 模拟存储，初始为空
    let saved = RefCell::new(None);
    let result = load(
        // 读取：返回存储里的值
        || Ok(saved.borrow().clone()),
        // 保存：写入存储并报告新增成功
        |id| {
            *saved.borrow_mut() = Some(id.into());
            Ok(true)
        },
        // 生成：返回新身份
        || NEW.into(),
    );
    // 返回的就是新生成的
    assert_eq!(result.unwrap(), NEW);
    // 且已保存
    assert_eq!(saved.into_inner(), Some(NEW.to_owned()));
}

#[test]
// 读取或写入失败时，不能声称拥有新身份
fn read_and_write_failures_cannot_claim_a_new_identity() {
    // 读取失败：报告存储不可用
    assert_eq!(
        load(
            || Err(Failure::StoreUnavailable),
            |_| Ok(true),
            || NEW.into()
        ),
        Err(Failure::StoreUnavailable)
    );
    // 断言
    assert_eq!(
        // 加载结果
        load(
            // 写入失败：报告写入失败，不声称拥有新身份
            || Ok(None),
            |_| Err(Failure::StoreWriteFailed),
            || NEW.into()
        ),
        Err(Failure::StoreWriteFailed)
    );
    assert_eq!(
        // 存储里的身份非法：报告身份无效
        load(|| Ok(Some("broken".into())), |_| Ok(true), || NEW.into()),
        Err(Failure::InvalidIdentity)
    );
}

#[test]
// 同时首次启动时，必须确认的是实际保存下来的那个身份
fn simultaneous_first_start_must_confirm_the_identity_that_was_saved() {
    assert_eq!(
        // 保存时发现别人已写入（返回 false），读回仍为空：冲突
        load(|| Ok(None), |_| Ok(false), || NEW.into()),
        Err(Failure::ConflictingIdentity)
    );
    // 另一种情形：保存返回 false，但读回的恰好是自己的身份
    let saved = RefCell::new(None);
    let result = load(
        || Ok(saved.borrow().clone()),
        |id| {
            *saved.borrow_mut() = Some(id.into());
            Ok(false)
        },
        || NEW.into(),
    );
    // 视为成功
    assert_eq!(result.unwrap(), NEW);
}
