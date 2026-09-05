import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createInterface } from "node:readline";
import type {
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";

type AwazState = "idle" | "listening" | "finalizing" | "speaking";

type AwazEvent =
  | { type: "ready"; version: string; provider: string }
  | { type: "capabilities"; stt: boolean; tts: boolean }
  | { type: "listen.started" }
  | { type: "listen.cancelled" }
  | { type: "transcript.partial"; text: string }
  | { type: "transcript.final"; text: string }
  | {
      type: "error";
      code: string;
      message: string;
      state: AwazState;
      fatal: boolean;
    }
  | { type: "shutdown" };

type UiContext = Pick<ExtensionContext, "ui">;

type VoiceState =
  | "booting"
  | "idle"
  | "starting"
  | "listening"
  | "finalizing"
  | "offline";

export default function awazExtension(pi: ExtensionAPI) {
  let child: ChildProcessWithoutNullStreams | undefined;
  let state: VoiceState = "offline";
  let generation = 0;
  let activeSession: ExtensionContext | undefined;
  let pendingListen = false;

  const setState = (next: VoiceState, ctx: UiContext) => {
    state = next;
    const label =
      next === "idle"
        ? "🎤 voice ready"
        : next === "starting"
          ? "🎤 starting…"
          : next === "listening"
            ? "🎤 listening"
            : next === "finalizing"
              ? "transcribing…"
              : next === "booting"
                ? "◌ voice starting"
                : "🎤 voice off";
    ctx.ui.setStatus("awaz", label);
  };

  const send = (message: unknown) => {
    if (!child || state === "offline" || state === "booting") return false;
    if (!child.stdin.writable) return false;
    child.stdin.write(`${JSON.stringify(message)}\n`);
    return true;
  };

  const stop = () => {
    const proc = child;
    child = undefined;
    generation += 1;
    state = "offline";
    pendingListen = false;

    if (!proc || proc.killed) return;
    if (proc.stdin.writable) {
      proc.stdin.end('{"type":"shutdown"}\n');
    }
    setTimeout(() => {
      if (!proc.killed) proc.kill();
    }, 250).unref();
  };

  const start = (sessionCtx: ExtensionContext) => {
    stop();
    const myGeneration = ++generation;
    const binary = process.env.AWAZ_BIN || "awaz";
    const args = ["serve"];
    if (process.env.AWAZ_MODEL_DIR) {
      args.push("--model-dir", process.env.AWAZ_MODEL_DIR);
    }
    if (process.env.AWAZ_LANGUAGE) {
      args.push("--language", process.env.AWAZ_LANGUAGE);
    }
    if (process.env.AWAZ_MODEL) {
      args.push("--model", process.env.AWAZ_MODEL);
    }
    if (process.env.AWAZ_DEVICE) {
      args.push("--device", process.env.AWAZ_DEVICE);
    }

    const proc = spawn(binary, args, { stdio: ["pipe", "pipe", "pipe"] });
    child = proc;
    proc.stdin.on("error", () => {
      // Process shutdown can race a final stdin write. The process lifecycle
      // handlers below are authoritative; an EPIPE here should not crash Pi.
    });
    setState("booting", sessionCtx);

    const isCurrent = () => generation === myGeneration && child === proc;
    const lines = createInterface({ input: proc.stdout });
    lines.on("line", (line) => {
      if (!isCurrent()) return;

      let event: AwazEvent;
      try {
        event = JSON.parse(line) as AwazEvent;
      } catch {
        return;
      }

      switch (event.type) {
        case "ready":
          setState("idle", sessionCtx);
          send({ type: "hello" });
          if (pendingListen) {
            pendingListen = false;
            setState("starting", sessionCtx);
            send({ type: "listen.start" });
          }
          break;
        case "listen.started":
          setState("listening", sessionCtx);
          break;
        case "transcript.partial":
          sessionCtx.ui.setStatus("awaz", `🎤 ${event.text.slice(-42)}`);
          break;
        case "transcript.final":
          if (event.text.trim()) {
            sessionCtx.ui.pasteToEditor(event.text.trim());
          }
          // Update after the editor mutation so Pi redraws the inserted text.
          setState("idle", sessionCtx);
          break;
        case "listen.cancelled":
          setState("idle", sessionCtx);
          break;
        case "error":
          if (event.fatal) {
            setState("offline", sessionCtx);
          } else if (
            event.state === "idle" ||
            event.state === "listening" ||
            event.state === "finalizing"
          ) {
            setState(event.state, sessionCtx);
          } else {
            setState("offline", sessionCtx);
          }
          sessionCtx.ui.notify(`Awaz: ${event.message}`, "error");
          break;
      }
    });

    proc.stderr.on("data", (buffer) => {
      if (!isCurrent() || state !== "booting") return;
      const text = String(buffer).trim();
      if (text) sessionCtx.ui.setStatus("awaz", `voice: ${text.slice(-48)}`);
    });

    proc.on("error", (error) => {
      if (!isCurrent()) return;
      setState("offline", sessionCtx);
      sessionCtx.ui.notify(
        `Could not start Awaz (${error.message}). Run 'awaz doctor'.`,
        "error",
      );
    });

    proc.on("exit", () => {
      if (!isCurrent()) return;
      child = undefined;
      setState("offline", sessionCtx);
    });
  };

  const toggle = (ctx: UiContext) => {
    if (state === "idle") {
      setState("starting", ctx);
      if (!send({ type: "listen.start" })) setState("offline", ctx);
      return;
    }

    if (state === "listening") {
      setState("finalizing", ctx);
      if (!send({ type: "listen.stop" })) setState("offline", ctx);
      return;
    }

    if (state === "booting") {
      ctx.ui.notify("Awaz is still loading the speech model.", "info");
    } else if (state === "offline") {
      if (!activeSession) {
        ctx.ui.notify("Awaz is not available in this session.", "info");
        return;
      }
      start(activeSession);
      pendingListen = true;
    }
  };

  pi.on("session_start", (_event, sessionCtx) => {
    activeSession = sessionCtx;
    // Awaz starts lazily on the first Alt+R or /awaz, not when the session opens.
  });
  pi.on("session_shutdown", () => {
    stop();
    activeSession = undefined;
  });

  pi.registerShortcut("alt+r", {
    description: "Awaz push-to-talk",
    handler: async (shortcutCtx) => toggle(shortcutCtx),
  });

  pi.registerCommand("awaz", {
    description: "Toggle Awaz push-to-talk (or /awaz cancel | unload)",
    handler: async (args, commandCtx) => {
      const arg = args?.trim();
      if (arg === "cancel") {
        if (state === "listening" || state === "starting" || state === "finalizing") {
          send({ type: "listen.cancel" });
        }
        return;
      }
      if (arg === "unload") {
        stop();
        commandCtx.ui.notify("Awaz voice unloaded.", "info");
        return;
      }
      toggle(commandCtx);
    },
  });
}
