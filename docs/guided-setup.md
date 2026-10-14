# Guided source setup on Ubuntu / WSL

New here? Follow [Your first Urdia task](quickstart.md) for the complete
Ubuntu/WSL path from downloading the source to checking a real task.

Already prepared the source folder? Run one command:

```sh
python3 -I -B scripts/setup-urdia.py
```

The assistant checks the source-build requirements, asks before compiling, lets
you choose an existing project or create an empty folder, then asks before starting
its local browser workspace. Click the private URL printed in the terminal. Keep
the terminal open; Ctrl+C stops the workspace. This first assistant runs in your
Ubuntu/WSL terminal; it is not a graphical Windows installer.

## What to choose

Start with **Offline demo** to check the interface without an account or project
edits. It is an echo demonstration, not AI. Rerun setup to choose **Qwen Code** if
installed, or **Codex** if its installed CLI matches the documented experimental
bridge version. See [Codex](codex.md) for the supported version. Setup never logs
in, inspects account credentials, installs agents, downgrades them or starts an
AI task. Finding a program does not verify account entitlement or provider quota.
Other reviewed ACP agents can use the existing [setup guide](workspace-ui.md).

For real agents, private launch/profile settings are enabled automatically so
**Setup guide** can save registrations. Settings live under the launcher's private
state directory, outside the selected project. Codex's generated launch definition
contains only executable paths and arguments, not keys. Re-running setup preserves
matching files; edited or redirected definitions are refused for explicit review.
An existing available Codex registration is reused instead of introducing a
duplicate provider. Review your saved registration in Setup guide. Choosing
another project uses its own settings directory.

Connect Jev or Laya in **Decision engines** after opening the workspace. No API key
is requested by this assistant. Jev connection tests require separate consent and
can consume quota. [Jev storage](jev-credentials.md) remains optional; Windows vault
acceptance is pending. Manual sending is default; [automatic Jev preview](automatic-jev.md) requires
connecting Jev, configured execution profiles and selecting that mode. Agents and approved
checks are not sandboxed.

## Requirements and recovery

The source preview baseline is Ubuntu24.04 x86_64, including Windows with WSL.
Missing tools are listed with a pointer to [installation](installation.md). Setup
does not run sudo, install system packages, download/run a Rust installer, copy a
binary globally, modify firewall rules or start a system service. Installing
prerequisites remains a separate administrator/user action.

After confirmation, Cargo builds the pinned source with `--release --locked`, at
most two build jobs, and the source tree's target directory. Public crate/toolchain
downloads may be needed. Build subprocesses receive a small environment allowlist;
this does not make third-party build scripts a sandbox. Build diagnostics are
transient and not saved in your project. If compilation fails, use the installation
guide and run its build command to diagnose, then rerun setup.

A read-only check opens no session or project settings and downloads nothing:

```sh
python3 -I -B scripts/setup-urdia.py --check
```

For an existing project, you can supply the folder and keep the other choices
interactive. Use a Linux/WSL path, not a Windows `C:\...` path:

```sh
python3 -I -B scripts/setup-urdia.py --workspace "$HOME/projects/my-project"
```

To reuse a build you explicitly trust, skip compilation. This does not verify the
binary is current; the ordinary path rebuilds first:

```sh
python3 -I -B scripts/setup-urdia.py --no-build
```

Advanced `--urdia` and demo `--fake-agent` overrides require `--no-build`.
Executable paths are passed as arguments, never evaluated by a shell. Setup does
not invoke an OS browser helper with the private token URL in process arguments.
Open the printed link yourself. No credentials or model calls are used by the
offline acceptance tests; native Windows interaction remains a separate test.
