//! Token totals, provider matching and balance projections shared by every UI.
mod agents;
mod formatting;
mod quota;
mod rows;

use serde_json::{Value, json};

fn argument(args: &[Value], index: usize) -> Result<&Value, String> {
    args.get(index)
        .ok_or_else(|| format!("Missing usage argument {index}"))
}

fn text(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .ok_or_else(|| "Usage argument must be a string".to_owned())
}

fn names(value: &Value) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| "Usage argument must be a string array".to_owned())?
        .iter()
        .map(|value| text(value).map(str::to_owned))
        .collect()
}

fn enabled(value: &Value) -> Result<Option<Vec<String>>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        names(value).map(Some)
    }
}

fn boolean(value: &Value) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| "Usage argument must be a boolean".to_owned())
}

fn used_tenths(value: &Value) -> Result<i64, String> {
    value
        .as_i64()
        .filter(|value| (0..=1_000).contains(value))
        .ok_or_else(|| "Quota usage must be integer tenths from 0 to 1000".to_owned())
}

/// Unknown operations belong to another feature; the shared bridge reports an
/// error only after every feature has had a chance to handle the request.
pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    let result = (|| {
        Ok(Some(match op {
            "usage.tokenCompact" => json!(formatting::token_compact(text(argument(args, 0)?)?)),
            "usage.tokenExact" => json!(formatting::token_exact(text(argument(args, 0)?)?)),
            "usage.amountLabel" => json!(formatting::amount_label(argument(args, 0)?)),
            "usage.hasNumericValue" => json!(formatting::has_numeric_value(argument(args, 0)?)),
            "usage.providerQuotaKey" => {
                json!(agents::provider_quota_key(text(argument(args, 0)?)?))
            }
            "usage.isEnabled" => json!(agents::is_enabled(
                text(argument(args, 0)?)?,
                enabled(argument(args, 1)?)?.as_deref()
            )),
            "usage.displayRank" => json!(agents::display_rank(text(argument(args, 0)?)?)),
            "usage.visibleIndices" => json!(agents::visible_indices(
                &names(argument(args, 0)?)?,
                enabled(argument(args, 1)?)?.as_deref()
            )),
            "usage.projectRows" => json!(agents::project_rows(
                boolean(argument(args, 0)?)?,
                &names(argument(args, 1)?)?,
                &names(argument(args, 2)?)?,
                enabled(argument(args, 3)?)?.as_deref()
            )),
            "usage.tokenText" => json!(agents::token_text(
                text(argument(args, 0)?)?,
                argument(args, 1)?.as_str(),
                boolean(argument(args, 2)?)?
            )),
            "usage.quotaLabel" => json!(quota::quota_label(text(argument(args, 0)?)?)),
            "usage.planLabel" => json!(quota::plan_label(argument(args, 0)?.as_str())),
            "usage.quotaWindows" => json!(quota::quota_windows(argument(args, 0)?)),
            "usage.kindLabel" => json!(quota::kind_label(text(argument(args, 0)?)?)),
            "usage.percentText" => json!(quota::percent_text(used_tenths(argument(args, 0)?)?)),
            "usage.quotaLevel" => json!(quota::quota_level(used_tenths(argument(args, 0)?)?)),
            "usage.displayRows" => json!(rows::display_rows(argument(args, 0)?)),
            "usage.cardProjection" => rows::card_projection(argument(args, 0)?),
            _ => return Ok(None),
        }))
    })();
    result.transpose()
}

#[cfg(test)]
#[path = "dispatch_test.rs"]
mod dispatch_test;
