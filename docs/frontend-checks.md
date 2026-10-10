# Check a static frontend in Urdia

With project verification enabled, finish a task, open **Check your project**, and
choose **Static frontend** or **Web counter**. Review the scope and execution
consent, then click **Run selected checks**. Desktop/mobile results and aggregate
errors appear in the panel; agent completion and independent evidence stay separate.
Changing the selected contract clears consent. Stop checks cancels the worker;
a result from an older task remains labeled historical.

## Two explicit contracts

- Static frontend: load index.html/local assets at 1280×800 and 390×844, observe
  script/resource errors and horizontal overflow. Four host check groups.
- Web counter: the same smoke plus visible #value initially 0, real buttons
  #increment/#decrement/#reset, increment by one, never negative, keyboard Enter
  and reset. Disabled decrement at zero is valid. Fourteen groups total.

This does not verify all application behavior, complete accessibility, security or
visual design. React/Vite/dev servers, build commands, installed packages and remote
APIs are not started by these contracts. Prepare a plain static root with index.html.
Relative and root-relative local assets work; other requests fail the check.
Include a responsive viewport declaration, for example
`<meta name="viewport" content="width=device-width, initial-scale=1">`.
A mobile page that keeps the desktop viewport fails the layout check.

## Optional browser runtime

Run once from the source folder:

```sh
python3 -I -B scripts/install-frontend-checks.py
```

The helper asks before downloading pinned Playwright 1.58.0 into your user directory
at ~/.local/share/urdia/frontend-checks. It never replaces an existing runtime,
installs system/browser packages, runs sudo, changes PATH or asks for an API key.
It needs Python's venv support. A separately installed Linux Chrome and a working
Chromium sandbox are required; default browser is /opt/google/chrome/chrome.
Windows Chrome is not this Linux/WSL worker. Unsupported/missing sandbox or runtime
produces no usable result; Urdia never disables the Chromium sandbox as a fallback.

Advanced trusted host configuration can set URDIA_FRONTEND_PYTHON to an absolute
Playwright-enabled Python executable and URDIA_FRONTEND_BROWSER to an absolute
Linux Chromium executable before starting Urdia. Browser/API callers cannot change
those executable paths. This optional installation is user-local server software,
not part of the core archive or a new model/provider integration.

## Scope and privacy

The checker fingerprints at most 128 regular static files (1 MiB each, 8 MiB total,
eight directory levels). HTML/CSS/JS/JSON/images/fonts are eligible; hidden files,
node_modules/vendor/venv and symlinks/hardlinked files are excluded or refused.
A required index.html and its eligible assets are served from an immutable memory
snapshot on ephemeral IP loopback, authenticated with a separate random header.
The page runs in a fresh browser context, with service workers/downloads blocked,
CSP and routing restricting resource requests to that snapshot. This is not a
complete OS/network or same-user isolation guarantee. Only run code you trust.

No source paths, contents, raw exceptions, console output, URLs/tokens, screenshots
or browser traces are returned by the check. The report contains aggregate counts,
fixed failure categories, timings and a source-set hash. Source changes cannot pass.
The outer Linux supervisor bounds runtime and cleans descendants on timeout or
cancellation. Browser profiles/helper reports stay temporary. No AI call is made.
Use the browser's own developer tools for detailed project debugging.
