# Launchboard: a real Jev-selected coding task

An actual Urdia task built this working, browser-local project board from an empty
workspace. [Try the app](https://urdia.tech/examples/launchboard/) or
[inspect the available build capture](https://urdia.tech/examples/launchboard/evidence.html).
The app is the task's output, not a replacement for Urdia's own workspace UI.

Jev selected **codex-medium** in **76 ms**, with codex-low still eligible as the
configured default. Codex requested **gpt-6.1-sol / medium** created exactly three
files through one explicitly approved edit batch. Submit-to-terminal wall time:
**166,699 ms**; host execution **166,187 ms**; first agent text **2,943 ms**.
The wall time includes observed operator approval handling (1,026 ms); startup,
credential connection and independent checks are separate. Actual remote model
inference, token usage and billed cost were not independently observed.

No operator edited the generated app source. Fifteen independent browser-check
groups passed, including creation, safe text rendering, keyboard focus, forward/
backward movement, combined filters, mutations while filtered, reload persistence,
JSON export, deletion persistence, reset confirmation, unrelated-storage retention,
empty/corrupt/duplicate/oversized stored data, unavailable storage and responsive
390/320px layouts. No external requests or uncaught browser errors occurred in the
acceptance run. Source hashes remained unchanged. See [all observed fields](observed.json).

## Run or inspect

[The exact task](task.txt) and unmodified [generated files](app/) are included.
From this example directory, serve only the app on loopback:

```sh
python3 -m http.server 8080 --bind 127.0.0.1 --directory app
```

Open `http://127.0.0.1:8080/`. The app makes no AI requests and needs no API key.
It keeps only this board in the browser's `urdia-launchboard-v1` storage key.
Export a JSON copy if you want to keep it; this is not a synced team service.

Optional developer acceptance requires separately installed Playwright and
sandbox-capable Chrome at `/opt/google/chrome/chrome` on Linux:

```sh
python3 check_browser.py /absolute/path/to/app /absolute/path/to/check-output
```

The checker executes browser JavaScript under a restrictive test CSP and records
video/screenshots. Review source first; Urdia itself is not an OS sandbox.
Browser viewport checks are not real-device or comprehensive accessibility audits.
An external tools-disabled Gemini 4 Argon review improved the independent checks;
Argon did not write or silently repair this app. No retries or hidden code fixes.
A bounded Qwen launch reported no selected auth type, so it was not admitted
as an execution-ready alternative; its probe made no inference call. This was
a launch-readiness observation, not an audit of native saved credentials.

This single successful project demonstrates selection, execution and a usable
result. It does not prove that medium was optimal, that low would fail, routing
savings, general reliability, autonomous planning or multi-provider superiority.

The available 163.76-second screen capture ends shortly before the final edit/
turn completion. Its container was remuxed without re-encoding or timeline edits
for seeking. It is not a complete recording; host completion and passing browser
checks are recorded separately in observed.json. No paid task was repeated just
to obtain a cleaner recording.
