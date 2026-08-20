import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";

// Runs `cargo clippy` after the agent edits or writes a `.rs` file.
//
// Fires on the `tool_result` event for the `edit` and `write` tools, once the
// file has been written to disk (a failed write yields isError and is skipped).
// The clippy run replaces the tool result content, so the agent sees the
// diagnostics and is pushed to fix them before the turn continues. Mirrors the
// CI lint gate exactly:
//   cargo clippy --all-targets --all-features -- -D warnings
const CLIPPY_ARGS = ["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"];

export default function clippyGate(pi: ExtensionAPI): void {
  pi.on("tool_result", async (event, ctx) => {
    // Only react to a successful write of a Rust source file.
    if (event.isError) return;
    if (event.toolName !== "edit" && event.toolName !== "write") return;

    // event.input is the raw tool arguments; `path` is the target file.
    const path = String(event.input?.path ?? "");
    if (!path.endsWith(".rs")) return;

    // Abort the clippy run if the agent is being torn down mid-turn.
    const res = await pi.exec("cargo", CLIPPY_ARGS, { cwd: ctx.cwd, signal: ctx.signal });

    // Cancelled: leave the result untouched.
    if (res.killed) return;

    const stdout = res.stdout.trim();
    const stderr = res.stderr.trim();
    // Clean run: no warnings and no output — nothing to surface.
    if (res.code === 0 && !stdout && !stderr) return;

    const lines = [`clippy after ${event.toolName} ${path}`, `exit=${res.code}`];
    if (stdout) lines.push("", "stdout:", stdout);
    if (stderr) lines.push("", "stderr:", stderr);

    return {
      content: [{ type: "text", text: lines.join("\n") }],
      details: { clippy: { path, exit: res.code } },
    };
  });
}
