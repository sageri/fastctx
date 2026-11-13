//! Cooperative stop checks that preserve completed replacement totals.

use crate::model::ToolResponse;
use crate::operation::OperationCtx;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::Duration;

pub(crate) const REPLACE_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn stop_reason(operation: &OperationCtx) -> Option<&'static str> {
    if operation.timed_out() {
        Some("timed out after 60 seconds")
    } else if operation.check().is_err() {
        Some("request cancelled")
    } else {
        None
    }
}

pub(crate) fn stopped_response(operation: &OperationCtx) -> ToolResponse {
    ToolResponse::text(format!(
        "(Partial: {}; 0 replacements written in 0 files. Verify actual file contents before retrying.)",
        stop_reason(operation).expect("a stopped request has a reason")
    ))
}

pub(super) fn lock<'a>(
    mutex: &'a Mutex<()>,
    operation: &OperationCtx,
) -> Result<MutexGuard<'a, ()>, String> {
    loop {
        operation
            .check()
            .map_err(|_| "Request stopped.".to_string())?;
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::WouldBlock) => {
                #[cfg(test)]
                operation.stage(crate::operation::TestStage::ReplaceLockWait);
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TryLockError::Poisoned(error)) => panic!("edit path lock poisoned: {error}"),
        }
    }
}
