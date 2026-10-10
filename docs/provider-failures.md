# Understanding an agent failure

A failed task now shows a fixed explanation in the live task panel and transcript.
The historical timeline retains an allowlisted category when one is available.
Native error bodies, keys, endpoints, paths and request details are not copied into
these explanations. Unknown causes remain unknown.

| Category | What it indicates | What to check before another task |
| --- | --- | --- |
| Authentication | The agent/provider reported a sign-in or API authentication problem | Use that agent's own sign-in/API settings; do not put a key into the conversation |
| Access denied | The provider reported insufficient access | Account/service/model access; this is separate from Urdia tool permissions |
| Usage or rate limit | The provider reported a quota, budget or request-rate limit | Provider limits and account status; Urdia does not purchase quota or retry automatically |
| Model unavailable | The provider reported an unsupported or unavailable model | Agent model settings and account access; a selector in a catalog is not an entitlement check |
| Connection | The agent/provider connection or service failed | Local agent process, network and provider availability |
| Timeout | An agent operation reached a deadline or reported a timeout | Current agent/workspace status before deciding to retry |
| Context limit | The provider reported excessive task context | Narrow the request and review any changed plan scope before sending |
| Invalid response | An adapter reported an incompatible protocol response | Supported agent version and adapter configuration |
| Unknown | The transport could not recognize a safe category | Agent-owned diagnostics in its own terminal; never paste private bodies or credentials |

Categories are diagnostic reports, not verified account state or proof that no file
changed. Some are inferred from bounded provider error phrases. Inspect the actual
workspace and independent tests after a failed task, especially if it had already
started using tools. Model prose and error categories do not grant permissions.

Urdia starts no extra task, changes no model/profile, and grants no permission when
a failure is recorded. Sending another task is explicit. A failed plan stage remains
unreviewed and offers a retry draft; it cannot unlock later stages by itself.
Existing cancellation behavior is unchanged; an error during an acknowledged
cancellation is not relabeled as an account problem.

Some agents close their session on failure (including the native Codex bridge).
After fixing the external cause, use **Recover your agent → Start a new agent
conversation → Reconnect agent** in the task panel. This appears after an original
agent failure in a non-trusted host. It replaces that session with the same loaded
provider, workspace and requested startup controls. Your host plan, files, task
history, independent check evidence and composer draft remain. No earlier
transcript or failed prompt is forwarded. Send the next task explicitly; for plan
work, prepare the stage again so its agreed objective/scope/checks are included.

Reconnection requires an idle task/verification queue and no pending permissions.
Duplicate delivery of the same pending revision starts only one replacement;
stale requests cannot replace a later session. Startup has a 60-second host ceiling
and remains interruptible with Ctrl+C. A failed startup displays only a fixed
category, blocks tasks on the disconnected original agent and requires renewed
consent to retry. At most 16 attempts are admitted per Urdia process.

Trusted-session hosts do not expose reconnection: restart explicitly and renew
trust rather than carrying a scope into a new native session. You may alternatively
select a configured fresh profile, whose tasks always create separate sessions.
If restarting the host, export the reviewed plan first and recover it with renewed
review. New environment values set in another terminal are not loaded into an
already running host; restart for launch-environment changes. Reconnection does
not fix credentials, account access or quota and is not an inference readiness
probe. External agents may perform their own network work during startup.

`urdia doctor` checks executable availability without inspecting credentials.
A found executable or advertised model does not demonstrate account readiness.
No background account probe or paid readiness request is introduced here. The
previous Qwen authentication failure has not been retested or fixed by this change.

For adapter details see [provider authoring](provider-authoring.md) and
[ADR 0039](adr/0039-safe-provider-failure-diagnostics.md) and
[ADR 0041](adr/0041-explicit-original-agent-reconnection.md).
