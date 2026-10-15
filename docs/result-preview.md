# Open your web result

After a real agent finishes a task, Linux/WSL users can open a static web project
from **Conversation → Open your web result** without starting a server manually.

1. Review the project files. The workspace must contain `index.html` at its root.
2. Confirm that you trust this browser code, then choose **Prepare preview**.
3. Choose **Open result**. The captured project opens in a separate tab.
4. Choose **Stop preview** when finished, and close its tab.

Preparation calls no AI model, installs nothing, and runs no build or project
command. Viewing the page executes its JavaScript in your browser. Use trusted
projects only: this is not a security sandbox or a guarantee of safe code.

## What you are seeing

The host captures the current static files when you prepare the preview. Its
fingerprint identifies that captured selection, not proof that the last agent
created every file. Changes made afterward do not appear until you prepare a new
preview. **Earlier task** means a newer task has run; it does not silently refresh
the older capture. Passing project checks is a separate action and result.

Each preparation uses a fresh private `*.localhost` address and port, separate
from Urdia's authenticated interface. Keep the full link private. One preview is
active at a time; preparing another stops the previous link. The link expires
after 15 minutes or when Urdia stops. Stopping the link cannot undo code already
loaded in a browser tab; close that tab too.

Browser data belongs to that temporary origin. A new preview does not inherit its
local storage. Export anything you want to keep before replacing the preview.
Urdia does not copy browser data back into the project.

## Supported projects and troubleshooting

This first version serves HTML, CSS, JavaScript, SVG, PNG/JPEG/ICO and WOFF fonts.
It supports root-relative local asset URLs. JSON/config files, source maps and
hidden files are excluded. External fonts, scripts, embedded frames and network
APIs are restricted; sites depending on them may not work. These restrictions are
not complete network isolation, including browser navigation.

No framework build, backend, package installation or dev server is started. Build
such a project yourself and use an appropriate trusted viewer for now. Preview
capture is available on Linux, including WSL; native Windows/macOS capture is not
implemented. Your browser must resolve reserved `*.localhost` names to loopback.
If it cannot, review local browser/proxy settings rather than exposing Urdia to
the network.

Capture fails closed for symlinks (including workspace ancestors), hard-linked
selected files, nonregular files or changing/oversized input. Unignored symlinks
or invalid names elsewhere in the traversed tree can also prevent preparation.
Hidden directories, `node_modules`, `vendor`, `venv`, `__pycache__` and `target`
are skipped. Limits: 128 selected files, 1 MiB per file, 8 MiB total, eight nested
directory levels, and 4,096 visited entries. Keep a small plain static project at
the selected workspace root. At most 16 preview requests are admitted per session.

If delivery is uncertain, **Retry delivery** reuses the same request; it does not
capture again. A stopped, expired or superseded request cannot restart its link.
A new preparation needs fresh consent. Preparation and task/check admission are
serialized, while an already open preview does not block another task.

Technical rationale: [ADR0052](adr/0052-static-result-preview.md).
