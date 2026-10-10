# Urdia — core Linux x86_64 candidate

Urdia hosts interchangeable coding agents with explicit permissions and local
history. This candidate package contains the core host; install an ACP agent
separately. Provider accounts and credentials belong to that agent.

This package is still under review and is not an approved public release.
The initial target is Linux x86_64 with glibc and system SQLite. The
observed build baseline is Ubuntu 24.04 x86_64 (including Ubuntu 24.04 in WSL).
The candidate evidence records its actual glibc and SQLite requirements; native
Windows and other-host compatibility are not implied.

## Install and open your workspace

Keep the extracted package together: `bin/urdia`, LICENSE, NOTICE,
THIRD_PARTY_NOTICES.md and the complete third-party directory. Put its `bin`
directory on PATH if desired. Until then, run `./bin/urdia` from this package's
root, which avoids assuming a system-wide installation. No service is installed.
For verified extraction, backup, replacement and rollback, use the repository's
candidate installation guide rather than overwriting an existing installation.

```sh
./bin/urdia doctor
./bin/urdia providers
```

Doctor discovers executables; it does not verify sign-in or account quota. The
core package does not contain a coding agent, fake/demo agent, native Codex bridge,
Python interpreter, local decision model, or provider credential. Install a
supported agent separately and sign in through that agent. Qwen's ACP adapter is
registered in the host; other launches/profiles need reviewed configuration.

Once Qwen is installed and signed in, use a real existing project directory:

```sh
./bin/urdia chat --provider qwen \
  --workspace /absolute/path/to/project \
  --permissions-ui --verification-ui
```

Open the private loopback URL printed by the terminal and keep the terminal open.
Send one concrete task, review any permission request, inspect the resulting files,
then explicitly run a suitable check. For multi-step work, review a project plan
and each step's result. **Setup guide** explains configuration; **Decision engines**
contains optional advice settings. A failed original agent can be explicitly
reconnected in a non-trusted host; that starts a fresh native conversation.
Ctrl+C stops this host. Never share the URL or put an API key in a task/screenshot.

Permissions default to deny-all. A browser chat listens on loopback only;
agent execution is not sandboxed. Prompt/response content is not persisted
unless explicitly requested. Automatic model routing remains disabled.

For source builds, configuration, backup/rollback, optional services, and provider
integration, see the project documentation at
https://github.com/SaulineytorX/urdia (private during development).
Do not launch with credentials unnecessary for your selected agent.

Optional stdlib Python project checks and fixed calculator checks are embedded
in Urdia and require Linux/WSL
with /usr/bin/python3 and /bin/kill. Enable --verification-ui explicitly and
review/confirm project Python execution in the private dashboard. It is not a
sandbox and does not verify arbitrary project requirements. Empty/skipped/failing
project suites cannot pass. No dependencies are installed by checks. Host times
are measured observations; missing token/cost data stays unreported.
