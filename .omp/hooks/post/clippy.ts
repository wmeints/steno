import type { HookAPI } from "@oh-my-pi/pi-coding-agent/extensibility/hooks";

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
        ["clippy", "--", "--format", "short"],
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

      const output = [result.stdout, result.stderr].filter(Boolean).join("\n").trim();
      if (!output) {
        return;
      }

      pi.sendMessage(
        {
          customType: "clippy-result",
          content: `cargo clippy found issues in ${path}:\n\n${output}`,
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
