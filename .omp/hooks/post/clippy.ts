import type { HookAPI } from "@oh-my-pi/pi-coding-agent/extensibility/hooks";
import path from "node:path";

// Parse cargo's short-format output into `file:line:col: error|warning`
// issue lines. Short format emits one line per issue; every other line
// (build progress, `error: could not compile ...` trailers) is dropped.
const SHORT_LINE = /^[^:\s][^:\n]*:\d+:\d+: (error|warning)/;

// Bare cargo failures (manifest parse errors, dependency resolution, build
// script crashes) print as `error: ...` with no file:line:col prefix.
const BARE_ERROR = /^(error|warning)(\[E\d+\])?: /;

function extractPath(event: { input: Record<string, unknown> }): string | undefined {
  const path = event.input.path;
  if (typeof path === "string") return path;
  const input = event.input.input;
  if (typeof input === "string") {
    const match = input.match(/^\[([^\]]+)#[0-9A-F]+]$/);
    if (match) return match[1];
  }
  return undefined;
}

// Cargo's short-format lines carry paths relative to the workspace root
// (the command's cwd). The tool-call path may be absolute or relative to
// something else, so compare by absolute resolved path.
function absPath(p: string, cwd: string): string {
  return path.resolve(cwd, p);
}

export default function (pi: HookAPI) {
  pi.on("tool_result", async (event, ctx) => {
    if (event.isError) return;
    if (event.toolName !== "write" && event.toolName !== "edit") return;

    const rawPath = extractPath(event);
    if (!rawPath || !/(?:rs|toml|lock)$/.test(rawPath)) return;

    try {
      const result = await pi.exec(
        "cargo",
        ["clippy", "--all-targets", "--message-format", "short"],
        { cwd: ctx.cwd, timeout: 30_000 },
      );

      if (result.killed || result.code === 124) {
        pi.sendMessage(
          {
            customType: "clippy-result",
            content: `cargo clippy timed out for ${rawPath}.`,
            display: true,
            attribution: "system",
          },
          { deliverAs: "followUp" },
        );
        return;
      }

      if (result.code === 0) {
        return;
      }

      const edited = absPath(rawPath, ctx.cwd);
      const output = [result.stdout, result.stderr].filter(Boolean).join("\n");
      const lines = output.split("\n");

      const issues = lines.filter((line) => {
        if (!SHORT_LINE.test(line)) return false;
        const file = line.slice(0, line.indexOf(":"));
        return absPath(file, ctx.cwd) === edited;
      });

      // When nothing is attributed to the edited file but cargo still
      // failed, surface the bare errors (broken manifest, dependency build
      // failure) rather than silently claiming the edit is clean.
      const bare = issues.length === 0 ? lines.filter((line) => BARE_ERROR.test(line)) : [];

      const report = [...issues, ...bare].join("\n").trim();
      if (!report) {
        return;
      }

      pi.sendMessage(
        {
          customType: "clippy-result",
          content: `cargo clippy found issues in ${rawPath}:\n\n${report}`,
          display: true,
          attribution: "system",
        },
        { deliverAs: "followUp" },
      );
    } catch (err) {
      pi.logger.error?.(`clippy hook: ${String(err)}`);
    }
  });
}
