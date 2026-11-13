//! The default byte-preserving replacement route in the single `fastctx` server.

use crate::control::settings;
use crate::edit::ReplaceRequest;
use crate::model::ToolResponse;
use crate::operation::RequestWorkGuard;
use crate::server::FastCtxServer;
use crate::server_support::run_blocking_replace;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::service::RequestContext;
use rmcp::{RoleServer, tool, tool_router};
use std::sync::Arc;

#[cfg(test)]
#[path = "edit_server_tests.rs"]
mod tests;

#[tool_router(router = edit_tool_router, vis = "pub(crate)")]
impl FastCtxServer {
    #[tool(
        name = "replace",
        description = "Batch find-and-replace across a file or directory (Rust regex, same engine\nas grep; no lookaround). A reference to an undefined capture group is\nrejected before any write. To delete whole lines, include \\n in the\npattern. Matching is leftmost-first and non-overlapping; unlike grep,\n`^`/`$` anchor the whole file by default — use (?m) for per-line anchors.\nRespects .gitignore; skips .git and binaries; files whose encoding cannot\nbe determined are skipped and listed. Each file is written atomically with\na concurrent-modification check, preserving its original encoding, BOM, and\nline endings. Stops cooperatively after 60 seconds including queue time, or\non cancellation. An in-flight atomic commit or blocking I/O may finish later;\ncompleted writes remain. After a timeout, verify actual file contents with\ngrep or inspect_local_file before retrying. Serialize replacements on the same directory.\nThe last line states Complete or Partial.",
        annotations(
            title = "Batch replace file contents",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn replace(
        &self,
        Parameters(request): Parameters<ReplaceRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let deadline = std::time::Instant::now() + crate::edit::control::REPLACE_TIMEOUT;
        let (guard, operation) = RequestWorkGuard::new(context.id, context.ct);
        let operation = operation.with_deadline(deadline);
        let _activity = self.activity.request();
        let replace = self.replace.clone();
        let control_paths = self.session.control_paths.clone();
        let status_shell = self.shell.clone();
        run_blocking_replace(
            Arc::clone(&self.session),
            Arc::clone(&self.replace_permits),
            guard,
            operation,
            move || status_shell.background_status(None),
            move |operation| {
                let max_file_size_mib = match settings::load(&control_paths)
                    .and_then(|settings| settings.replace_file_limit_mib())
                {
                    Ok(limit) => limit,
                    Err(error) => {
                        return ToolResponse::error(format!(
                            "Cannot use replace settings: {error}. Repair the FastCtx configuration and retry."
                        ));
                    }
                };
                replace.replace_with_operation(request, max_file_size_mib, &operation)
            },
        )
        .await
    }
}
