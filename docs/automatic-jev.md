# Let Jev choose and run a task

This is an explicitly enabled experimental preview mode. Manual sending remains
the default. It does not demonstrate that the production routing-quality gate
has passed, and it does not allow Jev to grant tool permissions.

1. In **Setup guide**, register your installed agents, inspect their model/effort
   controls and save executable profiles. Apply saved settings with the indicated
   host restart. Each selectable profile needs a precise model selector and, if
   an effort label is configured, a precise effort selector.
2. In **Decision engines → Jev**, connect your key, or restore a saved key. The
   separately authorized connection test can consume quota. See
   [Jev credentials](jev-credentials.md).
3. In **Conversation → Who chooses the agent?**, choose **Jev chooses and runs**.
4. Write your task and click **Let Jev choose & run**. This authorizes sending that
   task and host-prepared profile metadata to TypeSafe, and starting its valid
   selected profile without another model-selection confirmation.

Urdia compares configured, executable profiles; it does not invent models, log
into agent accounts or install agents. Model/effort selectors are applied to a
fresh agent session and checked through the existing host execution path before
the task is sent. Profile labels, executable discovery and a successful Jev
connection do not prove quality, cost or account availability.

The panel shows routing status, selected profile, then the existing agent progress,
host times and permission requests. This mode starts one submitted task. It does
not generate a plan, loop over future steps, run concurrent tasks, retry a model
failure automatically or authorize independent project checks. Reviewed plan-step
drafts retain their exact host step binding; later steps still require result review.
The original/default session stays unchanged. Earlier conversations are not
automatically forwarded to the selected agent; include the context the task needs.

## Refusals and interrupted delivery

No valid selection, escalation, unavailable exact selectors, stale review, busy
execution or a provider failure leaves the draft available for review. A failed
agent task is not retried automatically. Tool permissions stay deny by default/
one-shot under their existing policy; routing does not activate trusted mode.

If a network response is lost, **Retry same delivery** reuses the same decision
and task identifiers. An admitted request/task is not started twice. Before task
preparation, **Stop routing** stops this tab from dispatching its eventual result;
the already admitted decision may still finish and consume quota. After execution
delivery becomes uncertain, inspect current task state or retry the same delivery.

Keep the tab open until task admission. Reload/navigation clears this tab's routing
automation and restores manual mode; it neither cancels an already admitted task
nor automatically resumes an unfinished chain. Mode, task draft and delivery IDs
are not persisted in browser storage. A new session does not inherit automatic consent.
Each click submits only the displayed task, not permission for future autonomous work.

## Current evidence and limits

The owner explicitly requested this automatic preview. ADR0015's adverse blind
routing results remain valid; they are not relabeled as a production pass.
[ADR0050](adr/0050-owner-opt-in-automatic-jev.md) records the bounded opt-in scope.
Jev can abstain when configured evidence is insufficient. Urdia does not promise
the cheapest or best model without measured evidence. Costs/tokens not reported
by an execution agent remain unknown. Linux/WSL is the current supported host.

Browser Jev profile selection now uses a separate English suitability-preference
question, with host permission and startup checks unchanged. Generic CLI routing
and connection probes retain their original judgment. See
[ADR0051](adr/0051-jev-coding-profile-preference.md). A real three-profile calculator
pilot passed independent tests; this was development/tuning evidence, not a blind
quality benchmark or proof of cheapest selection.
