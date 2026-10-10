# Reviewed project plans

In Conversation, open **Project plan**. Write a project objective and 1–8 ordered
steps, each with a title, scope/files and acceptance criteria. Click **Review plan**
to freeze that outline. Reviewing, adding steps and preparing drafts make no model
calls, execute no commands and grant no permissions.

For example, a tax calculator could have an arithmetic step, a front-end step and
an integration-review step. Define and review the actual rules and checks yourself.
You may write an outline or edit a proposal from your selected agent. Jev/Laya
remain separate profile advisers; they do not generate or dispatch this outline.

## Execute one reviewed step

1. Select and explicitly apply the desired loaded agent/profile using the existing
   next-task selector, if available. Configure additional agents through Setup guide.
2. Click **Prepare step draft**. It fills an empty composer with the objective,
   selected step, scope and acceptance criteria. Existing drafts are preserved.
3. Review the agent and text, then click **Send task**. Normal provider quota and
   ACP permissions apply. This uses the existing authenticated prompt endpoint;
   no separate plan executor or broader permissions are introduced.
4. Inspect the files, tests and result. A completed turn shows **Agent finished ·
   review the result**. Where appropriate, run the independent calculator checks.
5. Click **I reviewed this result** to unlock preparation of the next step. That
   means operator review, not independent verification or mathematical correctness.
   The next step still requires its own explicit send.

The host atomically binds the unchanged prepared draft to the admitted task UUID,
and shows its observed agent, host state, most recent attempt and elapsed time.
It does not parse model prose to decide whether a task succeeded. Failed/cancelled
turns offer **Prepare retry draft**, requiring another explicit send; they cannot
be marked reviewed through this board. Automatic chaining stays disabled.

Editing the prepared prompt makes it an ordinary **unlinked** task. The plan stays
ready to prepare its original step; no association or success is inferred from
similar words. Ordinary unprepared sends and ordinary task advice
are not automatically linked to this outline. Other authenticated tabs share the reviewed outline. Scope and acceptance are instructions in the prompt, not host file
allowlists or executable check definitions. The plan does not enforce those rules
against an agent or other API clients.

## Ask an engine to choose the profile for a step

Start Urdia with loaded execution profiles and `--advisory-ui --advisory-profiles`.
Use **Find a profile for this step** on a ready/retryable stage. This prepares the
exact reviewed stage text in Decision engines and associates its plan ID, revision
and stage index. It preserves an unrelated advice draft and sends no request.

Choose a configured engine (Jev, Laya or another supported engine), then explicitly
request advice. Hosted engines still require their remote-consent checkbox; the
canonical prompt includes the project objective, current stage, scope and acceptance.
Laya uses the already configured service. No key is copied from another project.
No installed model, agent or service is provisioned by preparing a stage.

If a profile is recommended without fallback, use **Review and run this task**.
The preview shows the bound stage, original prompt, profile and exact model/effort
selectors. **Confirm and run** launches one task on that profile and binds its
admitted UUID to the stage. ACP permission decisions remain separate. The default
profile is kept for future ordinary tasks. Editing the advice draft after the
request cannot change the host-retained preview; editing before a new request
makes that request ordinary/unlinked.

Urdia checks stage freshness before advice admission, preview and atomic execution.
Clearing the plan, another tab admitting its task, or completing/reviewing that stage
invalidates an unused ticket. A linked request from a stale view starts no new work.
A recommendation already in flight may finish after the plan changes; its result
cannot execute the stale stage. Existing exact request/confirmation retries and
five-minute ticket expiry apply. A resumed page can recover the advice association
and request a fresh preview; it never automatically confirms a task.

The board retains the observed agent and profile labels/model/effort with the latest
attempt. Labels are declared configuration, not proof of remote inference or
routing quality. An accepted task still needs result review before the next stage.
No engine generates the plan, automatically advances it or proves correctness.

## Save and recover between Urdia restarts

Use **Save plan file** to download `urdia-plan.json` to your PC. This is an explicit
private-content export: it includes the objective, step titles/scopes/acceptance,
source project name, export time and latest saved task IDs/status/agent/elapsed
notes with operator-review flags. It excludes conversation responses, profile
selectors, credentials, permission receipts and live confirmation tickets. Text
you put in the outline is still included. Keep the file outside Git; Urdia's own
checkout ignores `urdia-plan*.json`. No server file is written automatically.

After restarting, open **Recover a saved plan**, choose the JSON file and click
**Review saved plan file**. Selecting the file alone sends no request. Preview is
validated by the authenticated local host and is inert. It shows the saved project,
current project, outline and historical notes as file claims; they are not current
verification or proof of remote execution. Check compatibility and actual project
files/tests, including whether previous work really finished.

Choose which **original stage** to continue from (defaults to the first stage).
Earlier stages will be omitted from the new working board only after you explicitly
confirm reviewing the outline/current files. Changing the start stage clears that
confirmation. Clear an existing idle plan before recovery; an active task or check
blocks it. Click **Recover reviewed remaining stages** to create a new plan UUID,
with zero attempts, no tasks and no result reviews. Nothing is executed or approved
from the file. The first remaining stage is ready to prepare; later stages use the
normal explicit result-review gate. Existing composer/advice drafts are kept.

**Saved history · not current verification** keeps the original notes separate
from live progress and survives a browser refresh. Previously completed/reviewed
UUIDs never become current tasks or authorize execution. Old plan links and advisory
tickets cannot act on the new identity. Select/apply a current profile or ask a
configured engine for fresh advice, then explicitly send/confirm the new task.
Exact recovery mutation retries reuse their ID and create no duplicate plan.

Saving a continuation preserves the original stage order and inherited notes for
earlier or not-yet-rerun stages; those notes are labelled inherited. New observed
attempts replace that stage's previous latest note. This is a snapshot, not a
complete attempt ledger. Keep your original backup if you need older snapshots.
Files are schema version 1, up to 96 KiB, with the normal outline bounds and strict
field validation. Unsupported schemas, malformed/unknown fields or oversized
files are refused without changing the plan. Preview/composer state is tab-local;
recovery and a later save remain explicit. No automatic on-disk restore or new
filesystem permissions are introduced.

See [ADR 0037](adr/0037-portable-reviewed-plan-recovery.md).

## Uncertain delivery and limits

If a planned send has uncertain delivery, the board waits for the host to observe
the same UUID. Polling can bind an admitted task even if its HTTP response was lost.
Retrying the unchanged draft reuses the existing prompt ID. If the request never
arrived, use **Restore pending draft for retry** and send it again. This restoration
explicitly replaces the composer text with that pending draft. The UI blocks a
different planned send while its acknowledgement is unresolved. Busy/rejected
requests remain subject to the existing host's limits and revision checks.

Reviewed outlines, result reviews and latest task associations live in **Urdia's
current host session memory**. Refreshing or reopening an authenticated browser
recovers that state, including an active task. All authenticated tabs see the same
plan. Restarting Urdia clears it unless you explicitly saved a portable file to
review and recover later. There is no automatic host disk persistence. Unreviewed
form fields, unsent composer drafts and pending browser requests remain tab-local
and are lost on refresh. Sent task prompts follow existing content-retention
settings, and agents may keep their own records.

**Clear plan · keeps host tasks** clears the outline while idle without cancelling
tasks or clearing your composer. Urdia rejects plan changes during an active task
or verification. Create, clear and result-review requests require authentication,
a unique request ID and the current plan revision. Exact retries reuse the same
ID; a conflicting payload is refused. **Retry pending plan update** resolves an
uncertain mutation without repeating an accepted change. The host accepts at most
32 plan mutations per session (task admission uses its existing separate limit).

A planned send carries the plan ID, revision and step index. Under the same lock
as task admission, Urdia checks the exact reviewed prompt, prior stage reviews,
current step state and profile revision, then binds only a successfully queued
command. Two tabs cannot admit two linked tasks for the same step; a stale view
must prepare again. Completed bound task observations survive eviction from the
visible conversation history, so a result can still be explicitly reviewed.
Editing a prepared prompt remains an ordinary unlinked send. Nothing resumes or
sends automatically after refresh. Plan state does not grant file access,
permissions or trusted mode, or infer success from agent text.

Fields are bounded: objective 1024 characters, up to eight steps, titles 120,
scope and acceptance 2000 each, with a combined 64 KiB UTF-8 content cap.
Requests also obey the existing HTTP body limit. Unchanged idle boards retain their DOM nodes.
Content is rendered as plain text. Review stages do not run themselves in the
background, and a resumed browser does not automatically send work.

See [ADR 0035](adr/0035-host-session-project-plans.md) for the boundary decisions and
[verification](verification.md) for the exact supported calculator contracts.

## Start from your objective

In Conversation, open Project plan and describe what you want. Select **Ask my
agent for a plan**. Review the prepared message and selected agent, then send it
from the conversation composer. This uses the selected agent's quota; Jev/Laya
profile recommendations remain separate. The request asks for planning only, but
these instructions are not a sandbox. Normal permission controls still apply.

When the agent finishes, select **Review latest response as a plan**. Urdia checks
the outline format and displays a proposal. Select **Use this proposal** to replace
the editable outline, adjust its scope and checks, then select **Review plan**.
Accepting it starts no task. Each stage still uses explicit preparation, optional
profile advice, task submission and result review.

Malformed proposals leave your editor untouched. A newer task or plan revision
invalidates the preview; review a current response again. Only one JSON object or
a single whole JSON/plain fenced block is accepted, with no extra fields and the
normal 1–8 stage bounds. No automatic JSON repair or repeated paid requests occur.
Proposal previews and unaccepted outlines are lost on browser refresh.

Validation checks structure, not planning quality. Review missing requirements and
uncertainties yourself. See the [representative evaluation rubric](planning-evaluation.md)
and [ADR 0038](adr/0038-reviewed-agent-plan-proposals.md).

## Returning from a planning response

When the latest completed task used Urdia's planning-message prefix, a visible
**Review returned plan** action opens the plan panel and invokes the same strict
private preview as **Review latest response as a plan**. The prefix is a navigation
hint, not provenance or validity. Ordinary replies, active/failed turns and an
already accepted plan do not show the action. Nothing is parsed automatically.
Review the preview, explicitly use it to fill the editor, adjust the steps and
select **Review plan**. Invalid output preserves your existing outline. Starting
another turn invalidates the older preview; refresh restores the navigation hint
from the current in-memory conversation, not a saved unaccepted proposal.

## Maintainer acceptance without a person at the keyboard

The optional `scripts/check-plan-workflow-codex.py` test acts as a fixed reviewer
on a disposable existing Python project. It sends exactly three native Codex
requests: an ambiguous planning objective, a complete clarified objective, then
one explicitly admitted repair stage. It exercises the actual browser proposal
preview/editor/acceptance, host-session refresh and one-shot buttons. The test
reviewer simplifies the proposal into one fixed repair; generated text never
chooses approvals or automatically starts later tasks. This tests the reviewed
workflow, not autonomous orchestration.

Without `--live-account` it stops before inspecting executables or loading the
browser. Using that flag consumes the chosen agent account quota. It requires a
separately installed Playwright and local Chrome, native Codex 0.159.2, and a new
private output directory outside this checkout.

```sh
python3 -I -B scripts/check-plan-workflow-codex.py \
  --live-account \
  --urdia ./target/release/urdia \
  --codex "$HOME/.local/bin/codex" \
  --chromium /opt/google/chrome/chrome \
  --model gpt-6.1-sol --effort low \
  --output "$HOME/urdia-workflow-evidence-new"
```

The runner permits at most two **edit-category** one-shot approvals in the final
turn and rejects all requests in planning turns and all other categories. The
redacted browser cannot independently inspect native paths/diffs; these are not
host file allowlists. The fixture instructions and independent final file-set,
unchanged-test hash and arithmetic checks establish the observed result. No trusted
session, automatic retry, engine call or installed decision-service change occurs.
The native account/provider's logging remains outside the runner's control.

The initial faulty project must fail both its unchanged three-test suite and seven
additional arithmetic cases. Before running the edited module, its source must match the exact known one-line
repair; extra code is rejected. After the repair both must pass, the test file must
be byte-identical and only the expected two source files may exist. A completed
agent response alone never passes this check. Private synthetic responses and
screenshots remain outside Git; normal workspace tests run only the offline
opt-in/fixture/independent-check regressions. No credentials are read or copied.

## Carry answered requirements to another agent

Every prepared stage sends the reviewed objective, the current stage scope and
acceptance checks. Write answered requirements into those fields before accepting
the outline. A selected-profile task uses a fresh session: earlier conversation,
other stage replies and informal agreements are not copied. This avoids silently
sending unrelated private text to another agent/provider, but it means conversation
memory alone is insufficient. Files are shared through the workspace; their contents
are not automatically embedded in the handoff. Ask the stage to read only the files
it needs and define what must remain unchanged.

For example, keep the agreed function signature, input types, numeric semantics,
allowed file set and independent checks in the objective/scope. An earlier agent's
unreviewed statement that tests passed is not a replacement for those requirements
or for host checks. Choose/apply the next agent, prepare the step and review the
exact outgoing message. Selection starts no task; result review unlocks the next
step but does not send it.

The optional `scripts/check-agent-handoff.py` performs two fixed requests in a
new disposable project, using actual browser controls. Normal CI uses only offline
checks. To test two native Codex sessions with distinct effort:

```sh
python3 -I -B scripts/check-agent-handoff.py \
  --live-account --diagnostic-agent codex --diagnostic-effort low \
  --urdia ./target/release/urdia \
  --codex "$HOME/.local/bin/codex" \
  --chromium /opt/google/chrome/chrome \
  --model gpt-6.1-sol --effort medium \
  --output "$HOME/urdia-handoff-evidence-new"
```

For Qwen diagnosis followed by Codex repair, use `--diagnostic-agent qwen` and
`--qwen /path/to/qwen`. This requires Qwen 0.24.7 and its own working authentication;
its configured model/effort are reported, not chosen by this test. Codex stays pinned
to 0.159.2. No account keys are copied and no fallback/retry is automatic. If diagnosis
fails, the test stops before the repair request. A test-only Qwen launcher can retain
one fixed error category (authentication, rate/quota, model unavailable, access denied
or unclassified); it never saves raw provider errors. Categories are diagnostics,
not proof of credential validity or execution authority.

Both modes permit at most two edit-category one-shot decisions for the known final
repair and reject all approvals in diagnosis. The exact one-line source guard,
unchanged-test hash, final file set, three tests and seven additional arithmetic
cases independently check the observed result. A conversation-only sentinel must
not appear in the second prompt; separate lifecycle events and confirmed model/
effort show the fresh session. Tokens/cost remain unknown unless actually reported.
This validates a fixed reviewed handoff, not autonomous routing or all agent pairs.
