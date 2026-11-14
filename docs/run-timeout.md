# Foreground run timeout

When `run` reaches `timeout_ms`, it terminates the process tree and drains the
captured pipe. Execution has ended and cannot resume. `Partial` describes an
interrupted command, even when all captured output is shown; it does not promise
that the command produced all its intended output.

The response distinguishes no captured output, all captured lines shown, long-line
truncation, and a budget-limited head/tail window. Ring-buffer loss is reported
separately. No persistent log or `job_id` is created for a foreground run, so
omitted output cannot be recovered through `job_output` or pagination.

A larger timeout, `run_background`, or output redirection can be chosen for a
new invocation. Re-running a command can repeat its side effects. Background jobs
retain their existing persistent-log and `job_output` contract.
