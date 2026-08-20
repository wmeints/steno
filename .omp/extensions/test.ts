import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";

// Runs `cargo test` at the end of a turn that edited Rust source.
//
// On `turn_end`, if the turn's tool results include a successful `edit`/`write`
// of a `.rs` file, run `cargo test --all-targets --all-features` (matching CI).
// The run is detached so it never stalls the turn; failures are steered back
// into the agent on the next prompt so they get fixed.
const TEST_ARGS = ["test", "--all-targets", "--all-features"];

// True when the turn's tool results included a successful write of a `.rs` file.
function editedRustInTurn(toolResults: { toolName?: string; isError?: boolean; input?: { path?: unknown } }[]): boolean {
  return toolResults.some((tr) => {
    if (tr.isError) return false;
    if (tr.toolName !== "edit" && tr.toolName !== "write") return false;
    return String(tr.input?.path ?? "").endsWith(".rs");
  });
}

export default function cargoTestGate(pi: ExtensionAPI): void {
  pi.on("turn_end", async (event, ctx) => {
    if (!editedRustInTurn(event.toolResults)) return;

    // Detach: do not await, so the turn is never blocked by the test run.
    void runTestsAndReport(pi, ctx);
  });
}

async function runTestsAndReport(pi: ExtensionAPI, ctx: { cwd: string }) {
  try {
    const res = await pi.exec("cargo", TEST_ARGS, { cwd: ctx.cwd });
    if (res.killed) return;

    // Passing tests produce only the summary; surface failures, ignore noise.
    if (res.code === 0) return;

    const out = [res.stdout, res.stderr].filter(Boolean).join("\n").trim();
    const message = `cargo test failed (exit=${res.code}) after the last Rust edit. Run it and fix the failures.`;
    // Steer into the agent's next prompt so the failure is addressed.
    await pi.sendMessage(
      { customType: "cargo-test-failure", content: message, display: false, details: { exit: res.code } },
      { deliverAs: "nextTurn" },
    );
    if (out) {
      // Surface the raw output as a visible diagnostic the user can read too.
      await pi.sendMessage(
        {
          customType: "cargo-test-failure-output",
          content: `cargo test output:\n${out}`,
          display: true,
        },
        { deliverAs: "nextTurn" },
      );
    }
  } catch {
    // Never let a test run crash the session.
  }
}
