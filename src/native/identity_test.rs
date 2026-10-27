use super::*;
use std::cell::RefCell;

const OLD: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const NEW: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

#[test]
fn an_existing_identity_does_not_need_or_read_pairing_records() {
    let result = load(
        || Ok(Some(OLD.into())),
        |_| panic!("must not overwrite identity"),
        || panic!("must not replace identity"),
    );
    assert_eq!(result.unwrap(), OLD);
}

#[test]
fn a_missing_identity_is_generated_and_saved_before_returning() {
    let saved = RefCell::new(None);
    let result = load(
        || Ok(saved.borrow().clone()),
        |id| {
            *saved.borrow_mut() = Some(id.into());
            Ok(true)
        },
        || NEW.into(),
    );
    assert_eq!(result.unwrap(), NEW);
    assert_eq!(saved.into_inner(), Some(NEW.to_owned()));
}

#[test]
fn read_and_write_failures_cannot_claim_a_new_identity() {
    assert_eq!(
        load(
            || Err(Failure::StoreUnavailable),
            |_| Ok(true),
            || NEW.into()
        ),
        Err(Failure::StoreUnavailable)
    );
    assert_eq!(
        load(
            || Ok(None),
            |_| Err(Failure::StoreWriteFailed),
            || NEW.into()
        ),
        Err(Failure::StoreWriteFailed)
    );
    assert_eq!(
        load(|| Ok(Some("broken".into())), |_| Ok(true), || NEW.into()),
        Err(Failure::InvalidIdentity)
    );
}

#[test]
fn simultaneous_first_start_must_confirm_the_identity_that_was_saved() {
    assert_eq!(
        load(|| Ok(None), |_| Ok(false), || NEW.into()),
        Err(Failure::ConflictingIdentity)
    );
    let saved = RefCell::new(None);
    let result = load(
        || Ok(saved.borrow().clone()),
        |id| {
            *saved.borrow_mut() = Some(id.into());
            Ok(false)
        },
        || NEW.into(),
    );
    assert_eq!(result.unwrap(), NEW);
}
