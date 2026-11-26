# ADR-0060: VinciSkin starts its own injection host

- **Status:** Accepted (2026-11-26)
- **Date:** 2026-11-26
- **Crates:** `vinciskin-injector` (the host), the Tauri shell (the process it starts), the frontend
  (the CN page)
- **Related:** [ADR-0059](0059-the-manager-injects-a-skin-instead-of-writing-one.md), which chose
  cslol's legacy DLL and shipped `cslol-inj.exe` beside it.

## Context and problem statement

ADR-0059 shipped `cslol-inj.exe` as the host. Its `cslol_find` looks for a window titled
`League of Legends (TM) Client` anywhere on the desktop and returns that window's thread, without
checking which process owns it or whether the thread pumps messages. Users on Windows 10 reported
three shapes of failure, all at the same step:

- `Failed to hook: Timeout out while waiting for init! TID=…; sent=300; failed=0; …` — a thread
  that exists, has a message queue, and never reads it, so the DLL is never loaded into the game.
- `[LoaderDiag] OpenThread(THREAD_QUERY_LIMITED_INFORMATION) failed; Win32=87` — a thread id that
  is already gone by the time the host uses it.
- the loader exiting on its own, which the page shows as injection switching back off.

The DLL was never the problem: `cslol_hook(tid, timeout, step)` takes a thread id and does the rest
itself, and the DLL that lands in the game is the same file either way.

## Decision

The host is ours: `vinciskin-inj.exe`, built from `crates/vinciskin-injector`.

- It loads `cslol-dll-legacy.dll` from disk, calls `cslol_init`, `cslol_set_config` and
  `cslol_hook` through the DLL's own exports, and never modifies the file. **The DLL that enters
  the game process is byte for byte the one already shipped**, so its hash and its standing with
  the client's anti-cheat do not change.
- The target is chosen here: a process named `League of Legends.exe`, a top-level window it owns,
  and only once that window answers `WM_NULL` within a second. A window that does not answer is a
  thread the hook cannot reach, so it is skipped and looked for again.
- A failed hook is retried up to five times with a fresh target before the session reports the
  failure, instead of the host exiting on the first miss.
- The host prefixes its own progress `[inj]` and everything the DLL logged `[dll]`, and `patcher.rs`
  reads only those two prefixes. A diagnostic line is no longer mistaken for the verdict, which is
  how `OpenThread … Win32=87` came to be shown as the reason for a failure it did not cause.

## Consequences

- `cslol-inj.exe` is no longer shipped. A process of that name left by an earlier build is still
  found and ended, because it keeps its hook in whatever game it finds next.
- The host is 186 KB beside a 568 KB DLL, and the pair is embedded in the app the same way the old
  pair was, so a release build is still one file.
- Moving off the shipped host means owning its behaviour: a change in the DLL's exports breaks the
  build rather than a release, since every symbol is resolved by name at startup.
- The pragma-shared section the DLL uses to report its own init and to carry its log is what makes
  the ack visible across processes, so the host must load the DLL as a module, not by data file.
