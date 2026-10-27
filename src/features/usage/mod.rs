//! Token totals, provider matching and balance projections shared by every UI.
mod agents;
mod formatting;
mod quota;
mod rows;

use serde_json::Value;

/// Unknown operations belong to another feature; the shared bridge reports an
/// error only after every feature has had a chance to handle the request.
pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match op {
        "usage.cardProjection" => Some(
            args.first()
                .ok_or_else(|| "Missing usage argument 0".to_owned())
                .map(rows::card_projection),
        ),
        _ => None,
    }
}

#[cfg(test)]
#[path = "dispatch_test.rs"]
mod dispatch_test;
