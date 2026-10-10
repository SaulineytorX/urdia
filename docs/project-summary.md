# Project results

After reviewing a project plan, Conversation shows **Project results** above the
latest task. It updates from the authenticated host snapshot without calling an
AI or starting a task. **View task** opens the corresponding retained transcript.

The four metrics describe different observations:

- **Agent finished:** the latest linked attempt ended normally. This does not
  establish that its changes are correct.
- **You reviewed:** an explicit operator review was recorded for that step.
- **Task checks passed:** the latest retained check request for that step's latest
  attempt passed its fixed contract. Checks require their existing separate
  execution consent. The result does not authorize a step or verify the whole
  project.
- **Latest attempt time:** the sum of measured host durations for the latest
  linked attempt of each step, including waiting. It excludes earlier retries
  and verification time; the timed-step count exposes missing observations.

Rows show the agent, number of attempts, duration, review state and task-specific
check evidence. Test executions sum the host contract and project suite; they are
not a count of distinct tests. Missing durations/counts remain **Not observed**.
Tokens and cost remain **Not reported**; time and text length are not billing data.

A later task marks earlier checks as historical because the workspace may have
changed. Retrying a step uses a new task identity and cannot inherit the previous
attempt's check result. Rechecking the same task uses the most recently admitted
request, even when it was cancelled or unavailable: an earlier pass is not silently
substituted. A new check does not revoke or create the separate operator review.

**Work finished · review your results** means all current steps have completed
attempts but some await review. **All steps reviewed** records operator review;
it never means every check passed or the project is independently verified.

The summary is session memory. It uses the current plan and the host's bounded
16 check jobs. A cleared plan hides it while preserving task history. Saved plan
recovery shows archived history separately; it does not restore current task/check
authority. No new export format, persistent evidence store or release gate is
introduced. See [project plans](project-plans.md),
[workspace controls](workspace-ui.md), and [ADR 0042](adr/0042-observed-project-summary.md).
