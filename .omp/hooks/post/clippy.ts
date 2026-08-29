import type { HookAPI } from "@oh-my-pi/pi-coding-agent/extensibility/hooks";

// Parse cargo's short-format output into `file:line:col: error|warning`
// issue lines. Short format emits one line per issue; every other line
// (build progress, `error: could not compile ...` trailers) is dropped.
const SHORT_LINE = /^[^:\s][^:\n]*:\d+:\d+: (error|warning)/;

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

export default function (pi: HookAPI) {
  pi.on("tool_result", async (event, ctx) => {
    if (event.isError) return;
    if (event.toolName !== "write" && event.toolName !== "edit") return;

    const path = extractPath(event);
    if (!path || !/(?:rs|toml|lock)$/.test(path)) return;

    try {
      const result = await pi.exec(
        "cargo",
        ["clippy", "--message-format", "short"],
        { cwd: ctx.cwd, timeout: 30_000 },
      );

      if (result.killed || result.code === 124) {
        pi.sendMessage(
          {
            customType: "clippy-result",
            content: `cargo clippy timed out for ${path}.`,
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

      const output = [result.stdout, result.stderr].filter(Boolean).join("\n");
      const issues = output
        .split("\n")
        .filter((line) => SHORT_LINE.test(line) && line.startsWith(`${path}:`));
      if (issues.length === 0) {
        return;
      }

      pi.sendMessage(
        {
          customType: "clippy-result",
          content: `cargo clippy found issues in ${path}:\n\n${issues
            .join("\n")
            .trim()}`,
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
